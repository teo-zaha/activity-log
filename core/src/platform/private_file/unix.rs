//! Folders and files on macOS and Linux, through descriptors: `openat`/`mkdirat` relative to the
//! folder opened once, `renameat` between the two. What differs per system (which file systems
//! keep permissions, ACLs) is in the `macos` and `linux` modules.

use std::{
    ffi::{CStr, CString, OsStr},
    fs::{self, File},
    io::{self, Read},
    os::{
        fd::{AsFd, AsRawFd, BorrowedFd, FromRawFd, OwnedFd},
        unix::{
            ffi::OsStrExt,
            fs::{DirBuilderExt, OpenOptionsExt},
        },
    },
    path::{Path, PathBuf},
};

#[cfg(target_os = "linux")]
use super::linux as os;
#[cfg(target_os = "macos")]
use super::macos as os;
use super::{OwnerOnly, PrivateFileError, staging_name};

/// How often a staging folder is created again when its name was taken or something appeared
/// in it before it was verified.
const ATTEMPTS: usize = 8;
/// How often creating the lock file is tried again after a spurious ENOENT.
const LOCK_CREATE_RETRIES: usize = 8;
/// How deep removing a staging folder goes into folders an editor made inside.
const REMOVE_DEPTH: usize = 8;

pub(super) struct Folder {
    fd: OwnedFd,
    path: PathBuf,
}

pub(super) fn open_folder(path: &Path) -> io::Result<Folder> {
    let path = fs::canonicalize(path)?;
    let fd = open_dir(None, &c_name(path.as_os_str())?)?;
    Ok(Folder { fd, path })
}

impl Folder {
    pub(super) fn open_lock(&self, name: &str) -> io::Result<File> {
        let name = c_name(OsStr::new(name))?;
        let mode: libc::c_uint = 0o600;
        let open = || {
            owned(unsafe {
                libc::openat(
                    self.fd.as_raw_fd(),
                    name.as_ptr(),
                    libc::O_RDWR | libc::O_CREAT | libc::O_NOFOLLOW | libc::O_CLOEXEC,
                    mode,
                )
            })
        };
        // macOS (APFS, Darwin 25) can fail one of two `openat(O_CREAT)` calls that create the
        // same name at once with ENOENT. A few tries ride that out; a folder that really was
        // removed keeps failing and is reported.
        let mut opened = open();
        for _ in 0..LOCK_CREATE_RETRIES {
            match opened {
                Err(ref err) if err.kind() == io::ErrorKind::NotFound => opened = open(),
                _ => break,
            }
        }
        Ok(File::from(opened?))
    }

    pub(super) fn read(&self, name: &str) -> io::Result<Option<Vec<u8>>> {
        let name = c_name(OsStr::new(name))?;
        let fd = unsafe {
            libc::openat(
                self.fd.as_raw_fd(),
                name.as_ptr(),
                libc::O_RDONLY | libc::O_NOFOLLOW | libc::O_CLOEXEC,
            )
        };
        let mut file = match owned(fd) {
            Ok(fd) => File::from(fd),
            Err(err) if err.kind() == io::ErrorKind::NotFound => return Ok(None),
            Err(err) => return Err(err),
        };
        let mut bytes = Vec::new();
        file.read_to_end(&mut bytes)?;
        Ok(Some(bytes))
    }

    pub(super) fn read_owner_only(&self, name: &str) -> io::Result<Option<OwnerOnly>> {
        let name = c_name(OsStr::new(name))?;
        // Non-blocking, so a FIFO planted under the name cannot hold the open.
        let fd = unsafe {
            libc::openat(
                self.fd.as_raw_fd(),
                name.as_ptr(),
                libc::O_RDONLY | libc::O_NOFOLLOW | libc::O_CLOEXEC | libc::O_NONBLOCK,
            )
        };
        let mut file = match owned(fd) {
            Ok(fd) => File::from(fd),
            Err(err) if err.kind() == io::ErrorKind::NotFound => return Ok(None),
            Err(err) => return Err(err),
        };
        let stat = fstat(file.as_fd())?;
        let private = stat.st_mode & libc::S_IFMT == libc::S_IFREG
            && stat.st_uid == unsafe { libc::geteuid() }
            && stat.st_mode & 0o077 == 0
            && !os::has_acl(file.as_fd())?;
        if !private {
            return Ok(Some(OwnerOnly::Exposed));
        }
        let size = usize::try_from(stat.st_size).unwrap_or(0);
        let mut bytes = Vec::with_capacity(size.saturating_add(1));
        file.read_to_end(&mut bytes)?;
        Ok(Some(OwnerOnly::Private(bytes)))
    }

    pub(super) fn check_private(&self) -> Result<(), PrivateFileError> {
        if !folder_is_private(self.fd.as_fd())? {
            return Err(PrivateFileError::Unprotectable {
                location: self.path.clone(),
            });
        }
        os::check_file_system(self.fd.as_fd(), &self.path)
    }

    pub(super) fn staging(&self, target_name: &str) -> Result<Staging, PrivateFileError> {
        let unprotectable = || PrivateFileError::Unprotectable {
            location: self.path.clone(),
        };
        self.check_private()?;

        for _ in 0..ATTEMPTS {
            let name = staging_name(target_name);
            let folder_name = c_name(OsStr::new(&name))?;
            if unsafe { libc::mkdirat(self.fd.as_raw_fd(), folder_name.as_ptr(), 0o700) } != 0 {
                let err = io::Error::last_os_error();
                if err.kind() == io::ErrorKind::AlreadyExists {
                    continue;
                }
                return Err(err.into());
            }
            let folder = open_dir(Some(self.fd.as_fd()), &folder_name)?;
            let staging = Staging {
                path: self.path.join(&name),
                parent: self.fd.try_clone()?,
                folder,
                folder_name,
                target_name: c_name(OsStr::new(target_name))?,
                keep: false,
            };
            // Empty until verified, so nothing secret can leak in between; something another
            // account created meanwhile is found below and the folder is discarded.
            cvt(unsafe { libc::fchmod(staging.folder.as_raw_fd(), 0o700) })?;
            if os::clear_acl(staging.folder.as_fd()).is_err()
                || !owned_with_mode(staging.folder.as_fd(), 0o700)?
                || os::has_acl(staging.folder.as_fd())?
            {
                return Err(unprotectable());
            }
            if !entries(staging.folder.as_fd())?.is_empty() {
                continue;
            }
            return Ok(staging);
        }
        Err(io::Error::other("Could not create a private folder").into())
    }
}

pub(super) struct Staging {
    parent: OwnedFd,
    folder: OwnedFd,
    folder_name: CString,
    target_name: CString,
    path: PathBuf,
    keep: bool,
}

impl Staging {
    pub(super) fn create(&self, name: &str) -> io::Result<File> {
        let name = c_name(OsStr::new(name))?;
        let mode: libc::c_uint = 0o600;
        let fd = unsafe {
            libc::openat(
                self.folder.as_raw_fd(),
                name.as_ptr(),
                libc::O_WRONLY | libc::O_CREAT | libc::O_EXCL | libc::O_NOFOLLOW | libc::O_CLOEXEC,
                mode,
            )
        };
        let file = File::from(owned(fd)?);
        cvt(unsafe { libc::fchmod(file.as_raw_fd(), 0o600) })?;
        if !owned_with_mode(file.as_fd(), 0o600)? || os::has_acl(file.as_fd())? {
            return Err(io::Error::other(format!(
                "{} cannot be kept private",
                self.path.display()
            )));
        }
        Ok(file)
    }

    pub(super) fn open(&self, name: &str) -> io::Result<File> {
        let name = c_name(OsStr::new(name))?;
        let fd = unsafe {
            libc::openat(
                self.folder.as_raw_fd(),
                name.as_ptr(),
                libc::O_RDONLY | libc::O_NOFOLLOW | libc::O_CLOEXEC,
            )
        };
        Ok(File::from(owned(fd)?))
    }

    /// Nothing holds a created file here.
    pub(super) fn release(&self, _name: &str) {}

    pub(super) fn publish(&self, name: &str) -> io::Result<()> {
        let name = c_name(OsStr::new(name))?;
        cvt(unsafe {
            libc::renameat(
                self.folder.as_raw_fd(),
                name.as_ptr(),
                self.parent.as_raw_fd(),
                self.target_name.as_ptr(),
            )
        })?;
        // Best effort: makes the rename itself durable.
        unsafe { libc::fsync(self.parent.as_raw_fd()) };
        Ok(())
    }

    pub(super) fn path(&self) -> &Path {
        &self.path
    }

    pub(super) fn keep(mut self) -> PathBuf {
        self.keep = true;
        self.path.clone()
    }
}

impl Drop for Staging {
    /// Empties the folder through its handle, then removes its name from the parent only while
    /// that name still is this folder. A folder moved away meanwhile is left empty where it is.
    fn drop(&mut self) {
        if self.keep {
            return;
        }
        remove_entries(self.folder.as_fd(), REMOVE_DEPTH);
        let at_name = fstatat(self.parent.as_fd(), &self.folder_name);
        let folder = fstat(self.folder.as_fd());
        if let (Ok(at_name), Ok(folder)) = (at_name, folder)
            && at_name.st_dev == folder.st_dev
            && at_name.st_ino == folder.st_ino
        {
            unsafe {
                libc::unlinkat(
                    self.parent.as_raw_fd(),
                    self.folder_name.as_ptr(),
                    libc::AT_REMOVEDIR,
                )
            };
        }
    }
}

pub(super) fn open_owned_input(path: &Path) -> io::Result<Option<File>> {
    // A FIFO under the settings name cannot hold the open.
    let file = fs::OpenOptions::new()
        .read(true)
        .custom_flags(libc::O_NOFOLLOW | libc::O_NONBLOCK)
        .open(path)?;
    let stat = fstat(file.as_fd())?;
    if stat.st_mode & libc::S_IFMT != libc::S_IFREG
        || !owned_by_user(&file)?
        || stat.st_mode & 0o022 != 0
        || os::acl_grants_others_write(file.as_fd())?
    {
        return Ok(None);
    }
    Ok(Some(file))
}

pub(super) fn owned_by_user(file: &fs::File) -> io::Result<bool> {
    use std::os::unix::fs::MetadataExt;
    Ok(file.metadata()?.uid() == unsafe { libc::geteuid() })
}

pub(super) fn readable_by_others(path: &Path) -> io::Result<bool> {
    use std::os::unix::fs::MetadataExt;
    let meta = fs::metadata(path)?;
    Ok(meta.mode() & 0o044 != 0 || os::acl_grants_read(path)?)
}

pub(super) fn private_command(program: &OsStr) -> std::process::Command {
    use std::os::unix::process::CommandExt;
    let mut command = std::process::Command::new(program);
    // Runs in the child between fork and exec; `umask` is async-signal-safe.
    unsafe {
        command.pre_exec(|| {
            libc::umask(0o077);
            Ok(())
        });
    }
    command
}

pub(super) fn create_private_folder(folder: &Path) -> io::Result<()> {
    fs::DirBuilder::new()
        .recursive(true)
        .mode(0o700)
        .create(folder)
}

/// Owned by the current user or root, and no other account may create, rename or delete
/// entries in it.
fn folder_is_private(folder: BorrowedFd) -> io::Result<bool> {
    let stat = fstat(folder)?;
    let owner = stat.st_uid;
    let owned = owner == unsafe { libc::geteuid() } || owner == 0;
    Ok(owned && stat.st_mode & 0o022 == 0 && !os::acl_grants_others_write(folder)?)
}

fn owned_with_mode(fd: BorrowedFd, mode: libc::mode_t) -> io::Result<bool> {
    let stat = fstat(fd)?;
    Ok(stat.st_uid == unsafe { libc::geteuid() } && stat.st_mode & 0o7777 == mode)
}

fn fstat(fd: BorrowedFd) -> io::Result<libc::stat> {
    let mut stat = std::mem::MaybeUninit::<libc::stat>::uninit();
    cvt(unsafe { libc::fstat(fd.as_raw_fd(), stat.as_mut_ptr()) })?;
    Ok(unsafe { stat.assume_init() })
}

fn fstatat(folder: BorrowedFd, name: &CStr) -> io::Result<libc::stat> {
    let mut stat = std::mem::MaybeUninit::<libc::stat>::uninit();
    cvt(unsafe {
        libc::fstatat(
            folder.as_raw_fd(),
            name.as_ptr(),
            stat.as_mut_ptr(),
            libc::AT_SYMLINK_NOFOLLOW,
        )
    })?;
    Ok(unsafe { stat.assume_init() })
}

fn open_dir(at: Option<BorrowedFd>, path: &CStr) -> io::Result<OwnedFd> {
    let dir = at.map_or(libc::AT_FDCWD, |fd| fd.as_raw_fd());
    let fd = unsafe {
        libc::openat(
            dir,
            path.as_ptr(),
            libc::O_RDONLY | libc::O_DIRECTORY | libc::O_NOFOLLOW | libc::O_CLOEXEC,
        )
    };
    owned(fd)
}

/// Removes everything in `folder` through descriptors; folders inside up to `depth` levels.
fn remove_entries(folder: BorrowedFd, depth: usize) {
    for entry in entries(folder).unwrap_or_default() {
        if unsafe { libc::unlinkat(folder.as_raw_fd(), entry.as_ptr(), 0) } == 0 {
            continue;
        }
        if depth > 0
            && let Ok(inner) = open_dir(Some(folder), &entry)
        {
            remove_entries(inner.as_fd(), depth - 1);
            unsafe { libc::unlinkat(folder.as_raw_fd(), entry.as_ptr(), libc::AT_REMOVEDIR) };
        }
    }
}

/// The names in a folder, without `.` and `..`.
fn entries(folder: BorrowedFd) -> io::Result<Vec<CString>> {
    let fd = cvt(unsafe { libc::dup(folder.as_raw_fd()) })?;
    let dir = unsafe { libc::fdopendir(fd) };
    if dir.is_null() {
        let err = io::Error::last_os_error();
        unsafe { libc::close(fd) };
        return Err(err);
    }
    // A duplicate shares the read position; start from the beginning.
    unsafe { libc::rewinddir(dir) };
    let mut names = Vec::new();
    loop {
        let entry = unsafe { libc::readdir(dir) };
        if entry.is_null() {
            break;
        }
        let name = unsafe { CStr::from_ptr((*entry).d_name.as_ptr()) };
        if name.to_bytes() != b"." && name.to_bytes() != b".." {
            names.push(name.to_owned());
        }
    }
    unsafe { libc::closedir(dir) };
    Ok(names)
}

fn c_name(path: &OsStr) -> io::Result<CString> {
    CString::new(path.as_bytes())
        .map_err(|_| io::Error::new(io::ErrorKind::InvalidInput, "a path with a NUL byte"))
}

fn owned(fd: libc::c_int) -> io::Result<OwnedFd> {
    let fd = cvt(fd)?;
    Ok(unsafe { OwnedFd::from_raw_fd(fd) })
}

fn cvt(result: libc::c_int) -> io::Result<libc::c_int> {
    if result < 0 {
        Err(io::Error::last_os_error())
    } else {
        Ok(result)
    }
}
