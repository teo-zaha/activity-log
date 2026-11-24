//! Windows: the staging folder and its files are created with a protected DACL whose single ACE
//! grants the current user full access, so nothing inherited from the parent applies and every
//! file an editor creates inside inherits only that ACE; the result is read back and checked
//! before anything is written. Folders are held open without delete sharing, so they cannot be
//! renamed while in use, and every file opened by path is checked to be the folder's own entry
//! and no reparse point. A file is published with `NtSetInformationFile(FileRenameInformationEx)`
//! on the handle it was written through, relative to the parent's handle: the Win32
//! `SetFileInformationByHandle` refuses a `RootDirectory` with `ERROR_INVALID_PARAMETER`.

use std::{
    ffi::c_void,
    fs::{self, File},
    io::{self, Read},
    os::windows::io::{AsRawHandle, FromRawHandle, OwnedHandle},
    path::{Path, PathBuf},
    sync::Mutex,
};

use windows::{
    Wdk::Storage::FileSystem::{
        FILE_INFORMATION_CLASS, FILE_RENAME_INFORMATION, FILE_RENAME_INFORMATION_0,
        FILE_RENAME_POSIX_SEMANTICS, FILE_RENAME_REPLACE_IF_EXISTS, FileRenameInformation,
        FileRenameInformationEx, NtSetInformationFile,
    },
    Win32::{
        Foundation::{
            HANDLE, HLOCAL, LocalFree, NTSTATUS, RtlNtStatusToDosError,
            STATUS_INVALID_DEVICE_REQUEST, STATUS_INVALID_INFO_CLASS, STATUS_INVALID_PARAMETER,
            STATUS_NOT_SUPPORTED,
        },
        Security::{
            ACCESS_ALLOWED_ACE, ACE_HEADER, ACL, ACL_SIZE_INFORMATION, AclSizeInformation,
            Authorization::{
                ConvertSidToStringSidW, ConvertStringSecurityDescriptorToSecurityDescriptorW,
                GetNamedSecurityInfoW, GetSecurityInfo, SDDL_REVISION_1, SE_FILE_OBJECT,
            },
            DACL_SECURITY_INFORMATION, GetAce, GetAclInformation, GetSecurityDescriptorControl,
            GetTokenInformation, OWNER_SECURITY_INFORMATION, PSECURITY_DESCRIPTOR, PSID,
            SE_DACL_PROTECTED, SECURITY_ATTRIBUTES, TOKEN_QUERY, TOKEN_USER, TokenUser,
        },
        Storage::FileSystem::{
            BY_HANDLE_FILE_INFORMATION, CREATE_NEW, CreateDirectoryW, CreateFileW, DELETE,
            FILE_ADD_FILE, FILE_ADD_SUBDIRECTORY, FILE_ATTRIBUTE_DIRECTORY, FILE_ATTRIBUTE_NORMAL,
            FILE_ATTRIBUTE_REPARSE_POINT, FILE_CREATION_DISPOSITION, FILE_DISPOSITION_FLAG_DELETE,
            FILE_DISPOSITION_FLAG_IGNORE_READONLY_ATTRIBUTE, FILE_DISPOSITION_FLAG_POSIX_SEMANTICS,
            FILE_DISPOSITION_INFO, FILE_DISPOSITION_INFO_EX, FILE_DISPOSITION_INFO_EX_FLAGS,
            FILE_FLAG_BACKUP_SEMANTICS, FILE_FLAG_OPEN_REPARSE_POINT, FILE_FLAGS_AND_ATTRIBUTES,
            FILE_GENERIC_READ, FILE_GENERIC_WRITE, FILE_NAME_NORMALIZED, FILE_SHARE_DELETE,
            FILE_SHARE_MODE, FILE_SHARE_READ, FILE_SHARE_WRITE, FileDispositionInfo,
            FileDispositionInfoEx, GETFINALPATHNAMEBYHANDLE_FLAGS, GetFileInformationByHandle,
            GetFinalPathNameByHandleW, GetVolumeInformationByHandleW, OPEN_ALWAYS, OPEN_EXISTING,
            SetFileInformationByHandle, VOLUME_NAME_DOS,
        },
        System::{
            IO::IO_STATUS_BLOCK,
            Threading::{GetCurrentProcess, OpenProcessToken},
        },
    },
    core::{HSTRING, PWSTR},
};

use super::{
    Found, OwnerOnly, PrivateFileError,
    acl::{self, AceView, Expected},
    removable, staging_name,
};

const FILE_PERSISTENT_ACLS: u32 = 0x0000_0008;
const ERROR_FILE_NOT_FOUND: u32 = 2;
const ERROR_PATH_NOT_FOUND: u32 = 3;
const ERROR_INVALID_PARAMETER: u32 = 87;
const ERROR_ALREADY_EXISTS: u32 = 183;
const ERROR_FILE_EXISTS: u32 = 80;
/// Rights that let another account add, rename or delete entries of a folder, or change its
/// protection: FILE_ADD_FILE, FILE_ADD_SUBDIRECTORY, FILE_DELETE_CHILD, DELETE, WRITE_DAC,
/// WRITE_OWNER, GENERIC_ALL and GENERIC_WRITE.
const WRITE_RIGHTS: u32 =
    0x2 | 0x4 | 0x40 | 0x1_0000 | 0x4_0000 | 0x8_0000 | 0x1000_0000 | 0x4000_0000;
/// FILE_READ_DATA, GENERIC_ALL and GENERIC_READ.
const READ_RIGHTS: u32 = 0x1 | 0x1000_0000 | 0x8000_0000;
const ATTEMPTS: usize = 8;

pub(super) struct Folder {
    /// Opened without delete sharing: nobody can rename or remove the folder meanwhile.
    handle: OwnedHandle,
    path: PathBuf,
    /// Where the handle really is, for checking files opened by path.
    final_path: PathBuf,
}

pub(super) fn open_folder(path: &Path) -> io::Result<Folder> {
    let path = fs::canonicalize(path)?;
    let handle = open_dir(
        &path,
        FILE_GENERIC_READ.0 | FILE_ADD_FILE.0 | FILE_ADD_SUBDIRECTORY.0,
    )?;
    let final_path = final_path(&handle)?;
    Ok(Folder {
        handle,
        path,
        final_path,
    })
}

impl Folder {
    pub(super) fn open_lock(&self, name: &str) -> io::Result<File> {
        let handle = open_entry(
            &self.path.join(name),
            &self.final_path,
            FILE_GENERIC_READ.0 | FILE_GENERIC_WRITE.0,
            OPEN_ALWAYS,
        )?;
        Ok(File::from(handle))
    }

    pub(super) fn read(&self, name: &str) -> io::Result<Option<Vec<u8>>> {
        let opened = open_entry(
            &self.path.join(name),
            &self.final_path,
            FILE_GENERIC_READ.0,
            OPEN_EXISTING,
        );
        let mut file = match opened {
            Ok(handle) => File::from(handle),
            Err(err)
                if matches!(
                    code(&err),
                    Some(ERROR_FILE_NOT_FOUND | ERROR_PATH_NOT_FOUND)
                ) =>
            {
                return Ok(None);
            }
            Err(err) => return Err(err),
        };
        let mut bytes = Vec::new();
        file.read_to_end(&mut bytes)?;
        Ok(Some(bytes))
    }

    pub(super) fn read_owner_only(&self, name: &str) -> io::Result<Option<OwnerOnly>> {
        let opened = open_entry(
            &self.path.join(name),
            &self.final_path,
            FILE_GENERIC_READ.0,
            OPEN_EXISTING,
        );
        let handle = match opened {
            Ok(handle) => handle,
            Err(err)
                if matches!(
                    code(&err),
                    Some(ERROR_FILE_NOT_FOUND | ERROR_PATH_NOT_FOUND)
                ) =>
            {
                return Ok(None);
            }
            Err(err) => return Err(err),
        };
        let sid = user_sid()?;
        let private = with_security(&handle, |owner, dacl, _| {
            Ok(acl::trusted(&sid_string(owner)?, &sid)
                && !grants_others(dacl, &sid, READ_RIGHTS | WRITE_RIGHTS)?)
        })?;
        if !private {
            return Ok(Some(OwnerOnly::Exposed));
        }
        let mut file = File::from(handle);
        let size = usize::try_from(file.metadata()?.len()).unwrap_or(0);
        let mut bytes = Vec::with_capacity(size.saturating_add(1));
        file.read_to_end(&mut bytes)?;
        Ok(Some(OwnerOnly::Private(bytes)))
    }

    pub(super) fn check_private(&self) -> Result<(), PrivateFileError> {
        self.check_private_for(&user_sid()?)
    }

    fn check_private_for(&self, sid: &str) -> Result<(), PrivateFileError> {
        if !keeps_acls(&self.handle)? || !folder_is_private(&self.handle, sid)? {
            return Err(PrivateFileError::Unprotectable {
                location: self.path.clone(),
            });
        }
        Ok(())
    }

    pub(super) fn staging(&self, target_name: &str) -> Result<Staging, PrivateFileError> {
        let unprotectable = || PrivateFileError::Unprotectable {
            location: self.path.clone(),
        };
        let sid = user_sid()?;
        self.check_private_for(&sid)?;
        for _ in 0..ATTEMPTS {
            let path = self.path.join(staging_name(target_name));
            match create_folder(&path, &sid) {
                Ok(()) => {}
                Err(err)
                    if matches!(code(&err), Some(ERROR_ALREADY_EXISTS | ERROR_FILE_EXISTS)) =>
                {
                    continue;
                }
                Err(err) => return Err(err.into()),
            }
            // With DELETE, so the folder is removed through this handle, never by its path.
            let folder = open_dir(&path, FILE_GENERIC_READ.0 | DELETE.0)?;
            let staging = Staging {
                parent: self.handle.try_clone()?,
                final_path: final_path(&folder)?,
                folder: Some(folder),
                path,
                target_name: target_name.encode_utf16().collect(),
                sid,
                created: Mutex::default(),
                keep: false,
            };
            let folder = staging.folder.as_ref().expect("just opened");
            if staging.final_path.parent() != Some(self.final_path.as_path())
                || !only_user(folder, &staging.sid, Expected::Folder)?
            {
                return Err(unprotectable());
            }
            return Ok(staging);
        }
        Err(io::Error::other("Could not create a private folder").into())
    }
}

pub(super) struct Staging {
    parent: OwnedHandle,
    /// Held open without delete sharing until the folder is removed.
    folder: Option<OwnedHandle>,
    final_path: PathBuf,
    target_name: Vec<u16>,
    path: PathBuf,
    sid: String,
    /// The handles files were created through, kept for publishing them.
    created: Mutex<Vec<(String, OwnedHandle)>>,
    keep: bool,
}

impl Staging {
    pub(super) fn create(&self, name: &str) -> io::Result<File> {
        let descriptor = Descriptor::new(&format!("D:P(A;;FA;;;{})", self.sid))?;
        let handle = unsafe {
            CreateFileW(
                &HSTRING::from(self.path.join(name).as_path()),
                FILE_GENERIC_READ.0 | FILE_GENERIC_WRITE.0 | DELETE.0,
                FILE_SHARE_READ,
                Some(&descriptor.attributes()),
                CREATE_NEW,
                FILE_ATTRIBUTE_NORMAL | FILE_FLAG_OPEN_REPARSE_POINT,
                None,
            )
        }
        .map_err(win)?;
        let handle = unsafe { OwnedHandle::from_raw_handle(handle.0) };
        if final_path(&handle)?.parent() != Some(self.final_path.as_path())
            || !only_user(&handle, &self.sid, Expected::CreatedFile)?
        {
            return Err(io::Error::other(format!(
                "{} cannot be kept private",
                self.path.display()
            )));
        }
        let file = File::from(handle.try_clone()?);
        self.created
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
            .push((name.to_string(), handle));
        Ok(file)
    }

    /// Reads what an editor left: the file must still be one of this folder's, no link, and
    /// protected only for the user, as everything created inside inherits.
    pub(super) fn open(&self, name: &str) -> io::Result<File> {
        let handle = open_entry(
            &self.path.join(name),
            &self.final_path,
            FILE_GENERIC_READ.0,
            OPEN_EXISTING,
        )?;
        if !only_user(&handle, &self.sid, Expected::EditorFile)? {
            return Err(io::Error::other(format!(
                "{} is no longer private",
                self.path.join(name).display()
            )));
        }
        Ok(File::from(handle))
    }

    /// Closes the handle `name` was created through, so another program can write and replace
    /// it.
    pub(super) fn release(&self, name: &str) {
        self.created
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
            .retain(|(created, _)| created != name);
    }

    pub(super) fn publish(&self, name: &str) -> io::Result<()> {
        let created = self
            .created
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        let (_, file) = created
            .iter()
            .find(|(created, _)| created == name)
            .ok_or_else(|| {
                io::Error::new(
                    io::ErrorKind::NotFound,
                    format!("{name} was not created here"),
                )
            })?;
        rename(file, &self.parent, &self.target_name)
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
    /// Deletes the files inside and then the folder, all through handles: each file is opened
    /// by name and checked to be this folder's own plain file; the folder through the handle
    /// held since it was created. Anything else inside (a folder, a link, something that
    /// cannot be checked) keeps the whole folder, with one warning: nothing is removed by path.
    fn drop(&mut self) {
        self.created
            .get_mut()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
            .clear();
        let Some(folder) = self.folder.take() else {
            return;
        };
        if self.keep {
            return;
        }
        let entries = self.entries();
        let found = entries
            .iter()
            .map(|entry| match entry {
                Some(_) => Found::File,
                None => Found::Other,
            })
            .collect::<Vec<_>>();
        if !removable(&found) {
            crate::diagnostics::warn(
                "settings",
                format!(
                    "{} was left in place: it holds a folder or link Activity Log does not remove",
                    self.path.display()
                ),
            );
            return;
        }
        for file in entries.into_iter().flatten() {
            let _ = delete(&file);
        }
        let _ = delete(&folder);
    }
}

impl Staging {
    /// Each entry of the folder: its handle when it is a plain file of this folder, `None`
    /// for anything else.
    fn entries(&self) -> Vec<Option<OwnedHandle>> {
        let Ok(entries) = fs::read_dir(&self.path) else {
            return vec![None];
        };
        entries
            .map(|entry| {
                let path = entry.ok()?.path();
                let handle = open(
                    &path,
                    DELETE.0 | FILE_GENERIC_READ.0,
                    FILE_SHARE_READ | FILE_SHARE_WRITE | FILE_SHARE_DELETE,
                    OPEN_EXISTING,
                    FILE_FLAG_BACKUP_SEMANTICS | FILE_FLAG_OPEN_REPARSE_POINT,
                )
                .ok()?;
                let plain = attributes(&handle).ok()?
                    & (FILE_ATTRIBUTE_DIRECTORY.0 | FILE_ATTRIBUTE_REPARSE_POINT.0)
                    == 0;
                let here = final_path(&handle).ok()?.parent() == Some(self.final_path.as_path());
                (plain && here).then_some(handle)
            })
            .collect()
    }
}

/// Deletes the object behind `handle` (POSIX semantics: its name goes at once).
fn delete(handle: &OwnedHandle) -> io::Result<()> {
    let info = FILE_DISPOSITION_INFO_EX {
        Flags: FILE_DISPOSITION_INFO_EX_FLAGS(
            FILE_DISPOSITION_FLAG_DELETE.0
                | FILE_DISPOSITION_FLAG_POSIX_SEMANTICS.0
                | FILE_DISPOSITION_FLAG_IGNORE_READONLY_ATTRIBUTE.0,
        ),
    };
    let ex = unsafe {
        SetFileInformationByHandle(
            raw(handle),
            FileDispositionInfoEx,
            (&info as *const FILE_DISPOSITION_INFO_EX).cast(),
            size_of::<FILE_DISPOSITION_INFO_EX>() as u32,
        )
    }
    .map_err(win);
    match ex {
        // Windows before 10 1709: deleted when the last handle closes.
        Err(err) if code(&err) == Some(ERROR_INVALID_PARAMETER) => {
            let info = FILE_DISPOSITION_INFO { DeleteFile: true };
            unsafe {
                SetFileInformationByHandle(
                    raw(handle),
                    FileDispositionInfo,
                    (&info as *const FILE_DISPOSITION_INFO).cast(),
                    size_of::<FILE_DISPOSITION_INFO>() as u32,
                )
            }
            .map_err(win)
        }
        result => result,
    }
}

pub(super) fn open_owned_input(path: &Path) -> io::Result<Option<File>> {
    let handle = open(
        path,
        FILE_GENERIC_READ.0,
        FILE_SHARE_READ | FILE_SHARE_WRITE | FILE_SHARE_DELETE,
        OPEN_EXISTING,
        FILE_ATTRIBUTE_NORMAL | FILE_FLAG_OPEN_REPARSE_POINT,
    )?;
    if attributes(&handle)? & (FILE_ATTRIBUTE_DIRECTORY.0 | FILE_ATTRIBUTE_REPARSE_POINT.0) != 0 {
        return Ok(None);
    }
    let sid = user_sid()?;
    let safe = with_security(&handle, |owner, dacl, _| {
        Ok(sid_string(owner)? == sid && !grants_others(dacl, &sid, WRITE_RIGHTS)?)
    })?;
    Ok(safe.then(|| File::from(handle)))
}

pub(super) fn owned_by_user(file: &File) -> io::Result<bool> {
    // Opened for reading, which includes `READ_CONTROL`.
    let handle = OwnedHandle::from(file.try_clone()?);
    let sid = user_sid()?;
    with_security(&handle, |owner, _, _| Ok(sid_string(owner)? == sid))
}

pub(super) fn readable_by_others(path: &Path) -> io::Result<bool> {
    let sid = user_sid()?;
    let mut dacl: *mut ACL = std::ptr::null_mut();
    let mut descriptor = PSECURITY_DESCRIPTOR::default();
    let status = unsafe {
        GetNamedSecurityInfoW(
            &HSTRING::from(path),
            SE_FILE_OBJECT,
            DACL_SECURITY_INFORMATION,
            None,
            None,
            Some(&mut dacl),
            None,
            &mut descriptor,
        )
    };
    if status.0 != 0 {
        return Err(io::Error::from_raw_os_error(status.0 as i32));
    }
    let result = grants_others(dacl, &sid, READ_RIGHTS);
    unsafe { LocalFree(Some(HLOCAL(descriptor.0))) };
    result
}

/// Whether the file at `path` is protected with one full-access entry for the user, as the
/// files this module creates are.
#[cfg(test)]
pub(super) fn is_created_owner_only(path: &Path) -> io::Result<bool> {
    let handle = open(
        path,
        FILE_GENERIC_READ.0,
        FILE_SHARE_READ | FILE_SHARE_WRITE | FILE_SHARE_DELETE,
        OPEN_EXISTING,
        FILE_FLAG_OPEN_REPARSE_POINT,
    )?;
    only_user(&handle, &user_sid()?, Expected::CreatedFile)
}

pub(super) fn private_command(program: &std::ffi::OsStr) -> std::process::Command {
    std::process::Command::new(program)
}

pub(super) fn create_private_folder(folder: &Path) -> io::Result<()> {
    if folder.is_dir() {
        return Ok(());
    }
    if let Some(parent) = folder
        .parent()
        .filter(|parent| !parent.as_os_str().is_empty())
    {
        create_private_folder(parent)?;
    }
    match create_folder(folder, &user_sid()?) {
        // Another process created it first.
        Err(err) if matches!(code(&err), Some(ERROR_ALREADY_EXISTS | ERROR_FILE_EXISTS)) => Ok(()),
        result => result,
    }
}

/// A folder only the current user can open, and whose files inherit only that.
fn create_folder(path: &Path, sid: &str) -> io::Result<()> {
    let descriptor = Descriptor::new(&format!("D:P(A;OICI;FA;;;{sid})"))?;
    unsafe { CreateDirectoryW(&HSTRING::from(path), Some(&descriptor.attributes())) }.map_err(win)
}

fn open_dir(path: &Path, access: u32) -> io::Result<OwnedHandle> {
    let handle = open(
        path,
        access,
        FILE_SHARE_READ | FILE_SHARE_WRITE,
        OPEN_EXISTING,
        FILE_FLAG_BACKUP_SEMANTICS | FILE_FLAG_OPEN_REPARSE_POINT,
    )?;
    if is_reparse_point(&handle)? {
        return Err(io::Error::other(format!(
            "{} is a link, not a folder",
            path.display()
        )));
    }
    Ok(handle)
}

/// A file of `folder` opened by path: refused if it is a reparse point or not really in it.
fn open_entry(
    path: &Path,
    folder: &Path,
    access: u32,
    disposition: FILE_CREATION_DISPOSITION,
) -> io::Result<OwnedHandle> {
    let handle = open(
        path,
        access,
        FILE_SHARE_READ | FILE_SHARE_WRITE | FILE_SHARE_DELETE,
        disposition,
        FILE_ATTRIBUTE_NORMAL | FILE_FLAG_OPEN_REPARSE_POINT,
    )?;
    if is_reparse_point(&handle)? || final_path(&handle)?.parent() != Some(folder) {
        return Err(io::Error::other(format!(
            "{} is not a file of {}",
            path.display(),
            folder.display()
        )));
    }
    Ok(handle)
}

fn open(
    path: &Path,
    access: u32,
    share: FILE_SHARE_MODE,
    disposition: FILE_CREATION_DISPOSITION,
    flags: FILE_FLAGS_AND_ATTRIBUTES,
) -> io::Result<OwnedHandle> {
    let handle = unsafe {
        CreateFileW(
            &HSTRING::from(path),
            access,
            share,
            None,
            disposition,
            flags,
            None,
        )
    }
    .map_err(win)?;
    Ok(unsafe { OwnedHandle::from_raw_handle(handle.0) })
}

fn is_reparse_point(handle: &OwnedHandle) -> io::Result<bool> {
    Ok(attributes(handle)? & FILE_ATTRIBUTE_REPARSE_POINT.0 != 0)
}

fn attributes(handle: &OwnedHandle) -> io::Result<u32> {
    let mut info = BY_HANDLE_FILE_INFORMATION::default();
    unsafe { GetFileInformationByHandle(raw(handle), &mut info) }.map_err(win)?;
    Ok(info.dwFileAttributes)
}

/// Where a handle really is, with every link resolved.
fn final_path(handle: &OwnedHandle) -> io::Result<PathBuf> {
    let mut buffer = vec![0u16; 1024];
    loop {
        let length = unsafe {
            GetFinalPathNameByHandleW(
                raw(handle),
                &mut buffer,
                GETFINALPATHNAMEBYHANDLE_FLAGS(FILE_NAME_NORMALIZED.0 | VOLUME_NAME_DOS.0),
            )
        } as usize;
        if length == 0 {
            return Err(io::Error::last_os_error());
        }
        if length < buffer.len() {
            let text = String::from_utf16_lossy(&buffer[..length]).to_lowercase();
            return Ok(PathBuf::from(text));
        }
        buffer.resize(length + 1, 0);
    }
}

/// Whether the volume keeps ACLs: NTFS and ReFS do, FAT and exFAT do not.
fn keeps_acls(folder: &OwnedHandle) -> io::Result<bool> {
    let mut flags = 0u32;
    unsafe { GetVolumeInformationByHandleW(raw(folder), None, None, None, Some(&mut flags), None) }
        .map_err(win)?;
    Ok(flags & FILE_PERSISTENT_ACLS != 0)
}

/// Owned by the user, SYSTEM or Administrators, and no ACE lets another account add, rename
/// or delete entries or change the folder's protection.
fn folder_is_private(folder: &OwnedHandle, sid: &str) -> io::Result<bool> {
    with_security(folder, |owner, dacl, _| {
        Ok(acl::trusted(&sid_string(owner)?, sid) && !grants_others(dacl, sid, WRITE_RIGHTS)?)
    })
}

/// Whether the object's owner, protection and ACEs are what `expected` asks
/// ([`acl::owner_only`]).
fn only_user(handle: &OwnedHandle, sid: &str, expected: Expected) -> io::Result<bool> {
    with_security(handle, |owner, dacl, descriptor| {
        let owner = sid_string(owner)?;
        let (mut control, mut revision) = (0u16, 0u32);
        unsafe { GetSecurityDescriptorControl(descriptor, &mut control, &mut revision) }
            .map_err(win)?;
        let protected = control & SE_DACL_PROTECTED.0 != 0;
        let aces = aces(dacl)?;
        Ok(acl::owner_only(
            &owner,
            sid,
            protected,
            aces.as_deref(),
            expected,
        ))
    })
}

/// The owner, DACL and descriptor of `handle`, for `read`; freed afterwards.
fn with_security<T>(
    handle: &OwnedHandle,
    read: impl FnOnce(PSID, *mut ACL, PSECURITY_DESCRIPTOR) -> io::Result<T>,
) -> io::Result<T> {
    let mut owner = PSID::default();
    let mut dacl: *mut ACL = std::ptr::null_mut();
    let mut descriptor = PSECURITY_DESCRIPTOR::default();
    let status = unsafe {
        GetSecurityInfo(
            raw(handle),
            SE_FILE_OBJECT,
            OWNER_SECURITY_INFORMATION | DACL_SECURITY_INFORMATION,
            Some(&mut owner),
            None,
            Some(&mut dacl),
            None,
            Some(&mut descriptor),
        )
    };
    if status.0 != 0 {
        return Err(io::Error::from_raw_os_error(status.0 as i32));
    }
    let result = read(owner, dacl, descriptor);
    unsafe { LocalFree(Some(HLOCAL(descriptor.0))) };
    result
}

/// [`acl::grants_others`] for a DACL.
fn grants_others(dacl: *mut ACL, sid: &str, rights: u32) -> io::Result<bool> {
    let aces = aces(dacl)?;
    Ok(acl::grants_others(aces.as_deref(), sid, rights))
}

/// The ACEs of `dacl`; `None` without a DACL.
fn aces(dacl: *mut ACL) -> io::Result<Option<Vec<AceView>>> {
    if dacl.is_null() {
        return Ok(None);
    }
    let mut info = ACL_SIZE_INFORMATION::default();
    unsafe {
        GetAclInformation(
            dacl,
            (&mut info as *mut ACL_SIZE_INFORMATION).cast(),
            size_of::<ACL_SIZE_INFORMATION>() as u32,
            AclSizeInformation,
        )
    }
    .map_err(win)?;
    let mut aces = Vec::new();
    for index in 0..info.AceCount {
        let mut ace: *mut c_void = std::ptr::null_mut();
        unsafe { GetAce(dacl, index, &mut ace) }.map_err(win)?;
        let header = unsafe { &*(ace as *const ACE_HEADER) };
        // Every ACE form has its mask right after the header.
        let allowed = unsafe { &*(ace as *const ACCESS_ALLOWED_ACE) };
        let sid = if header.AceType == acl::ACCESS_ALLOWED {
            Some(sid_string(PSID(
                (&allowed.SidStart as *const u32).cast_mut().cast(),
            ))?)
        } else {
            None
        };
        aces.push(AceView {
            kind: header.AceType,
            flags: header.AceFlags,
            mask: allowed.Mask,
            sid,
        });
    }
    Ok(Some(aces))
}

#[cfg(test)]
thread_local! {
    /// Answers every POSIX rename as not supported, as a kernel or file system without them
    /// does, so the plain rename runs.
    pub(super) static REFUSE_POSIX_RENAME: std::cell::Cell<bool> =
        const { std::cell::Cell::new(false) };
}

/// Statuses of a `FileRenameInformationEx` request that a plain rename may still do: a kernel
/// without the class (before Windows 10 1607), or a file system without POSIX renames.
const RENAME_EX_UNSUPPORTED: [NTSTATUS; 4] = [
    STATUS_INVALID_INFO_CLASS,
    STATUS_NOT_SUPPORTED,
    STATUS_INVALID_DEVICE_REQUEST,
    STATUS_INVALID_PARAMETER,
];

/// Moves `file` to `name` in the folder `parent`, replacing what is there in one step, by
/// handles alone: `name` is resolved in `parent`, never by a path. A POSIX rename first, which
/// also replaces a target others hold open with delete sharing; where that is not offered, a
/// plain one, which such a target refuses.
fn rename(file: &OwnedHandle, parent: &OwnedHandle, name: &[u16]) -> io::Result<()> {
    let posix = FILE_RENAME_INFORMATION_0 {
        Flags: FILE_RENAME_REPLACE_IF_EXISTS | FILE_RENAME_POSIX_SEMANTICS,
    };
    #[cfg(test)]
    let posix_status = if REFUSE_POSIX_RENAME.get() {
        STATUS_NOT_SUPPORTED
    } else {
        set_rename(file, parent, name, FileRenameInformationEx, posix)?
    };
    #[cfg(not(test))]
    let posix_status = set_rename(file, parent, name, FileRenameInformationEx, posix)?;
    let refused = match posix_status {
        status if status.is_ok() => return Ok(()),
        status if RENAME_EX_UNSUPPORTED.contains(&status) => status,
        status => return Err(nt_error(status)),
    };
    // From zeroes, so the union's bytes past the flag are set too.
    let mut plain = FILE_RENAME_INFORMATION_0::default();
    plain.ReplaceIfExists = true;
    match set_rename(file, parent, name, FileRenameInformation, plain)? {
        status if status.is_ok() => Ok(()),
        status => {
            let plain = nt_error(status);
            Err(io::Error::new(
                plain.kind(),
                RenameRefused {
                    posix: nt_error(refused),
                    plain,
                },
            ))
        }
    }
}

/// One rename request: `NtSetInformationFile` with a `FILE_RENAME_INFORMATION` naming `name`
/// relative to `parent`.
fn set_rename(
    file: &OwnedHandle,
    parent: &OwnedHandle,
    name: &[u16],
    class: FILE_INFORMATION_CLASS,
    flags: FILE_RENAME_INFORMATION_0,
) -> io::Result<NTSTATUS> {
    let too_long = || io::Error::new(io::ErrorKind::InvalidInput, "The file name is too long");
    let name_bytes = std::mem::size_of_val(name);
    let size = size_of::<FILE_RENAME_INFORMATION>() + name_bytes;
    let name_length = u32::try_from(name_bytes).map_err(|_| too_long())?;
    let length = u32::try_from(size).map_err(|_| too_long())?;
    // u64 words: aligned for the structure (8, for its handle), zeroed, and `size` bytes or
    // more.
    let mut buffer = vec![0u64; size.div_ceil(8)];
    let info = buffer.as_mut_ptr().cast::<FILE_RENAME_INFORMATION>();
    let mut status_block = IO_STATUS_BLOCK::default();
    // SAFETY: `info` points at the start of `buffer`, aligned and large enough for the
    // structure, which is written whole; `FileName` begins inside it and the buffer holds
    // `name_bytes` more bytes past the structure's end, so the `name.len()` units copied there
    // stay inside it. `FileNameLength` counts those bytes (the NT call needs no terminator).
    // `name` is one checked entry name (`check_name`), as a relative rename needs. Both handles
    // are borrowed for the call only, which keeps neither; the file was opened for synchronous
    // I/O, so the call has finished with the buffer and the status block when it returns.
    let status = unsafe {
        info.write(FILE_RENAME_INFORMATION {
            Anonymous: flags,
            RootDirectory: raw(parent),
            FileNameLength: name_length,
            FileName: [0],
        });
        std::ptr::copy_nonoverlapping(
            name.as_ptr(),
            (&raw mut (*info).FileName).cast::<u16>(),
            name.len(),
        );
        NtSetInformationFile(
            raw(file),
            &mut status_block,
            info.cast_const().cast(),
            length,
            class,
        )
    };
    Ok(status)
}

/// An NT status as the Win32 error it maps to, so its kind is the usual one; a status without
/// a Win32 counterpart maps to `ERROR_MR_MID_NOT_FOUND` (317), never to success.
fn nt_error(status: NTSTATUS) -> io::Error {
    // SAFETY: a lookup on a value: no pointer is passed. Besides the code it returns it only
    // writes `status` into this thread's own `LastStatusValue` (in its TEB), which nothing
    // here reads.
    let code = unsafe { RtlNtStatusToDosError(status) };
    io::Error::from_raw_os_error(code as i32)
}

/// Both renames of [`rename`] were refused.
#[derive(Debug)]
pub(super) struct RenameRefused {
    pub(super) posix: io::Error,
    pub(super) plain: io::Error,
}

impl std::fmt::Display for RenameRefused {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "the POSIX rename was refused ({}), and so was the plain rename ({})",
            self.posix, self.plain
        )
    }
}

impl std::error::Error for RenameRefused {}

/// The current user's SID, as `S-1-5-21-…`.
fn user_sid() -> io::Result<String> {
    let mut token = HANDLE::default();
    unsafe { OpenProcessToken(GetCurrentProcess(), TOKEN_QUERY, &mut token) }.map_err(win)?;
    let token = unsafe { OwnedHandle::from_raw_handle(token.0) };
    let mut size = 0u32;
    let _ = unsafe { GetTokenInformation(raw(&token), TokenUser, None, 0, &mut size) };
    let mut buffer = vec![0u64; (size as usize).div_ceil(8)];
    unsafe {
        GetTokenInformation(
            raw(&token),
            TokenUser,
            Some(buffer.as_mut_ptr().cast()),
            size,
            &mut size,
        )
    }
    .map_err(win)?;
    let user = unsafe { &*buffer.as_ptr().cast::<TOKEN_USER>() };
    sid_string(user.User.Sid)
}

fn sid_string(sid: PSID) -> io::Result<String> {
    let mut text = PWSTR::null();
    unsafe { ConvertSidToStringSidW(sid, &mut text) }.map_err(win)?;
    let result = unsafe { text.to_string() }.map_err(|_| io::Error::other("A SID is not text"));
    unsafe { LocalFree(Some(HLOCAL(text.0.cast()))) };
    result
}

fn raw(handle: &OwnedHandle) -> HANDLE {
    HANDLE(handle.as_raw_handle())
}

/// A Windows error as the Win32 code it carries: `windows` reports them as HRESULTs.
pub(super) fn win(err: windows::core::Error) -> io::Error {
    let code = err.code().0 as u32;
    if code & 0xFFFF_0000 == 0x8007_0000 {
        io::Error::from_raw_os_error((code & 0xFFFF) as i32)
    } else {
        io::Error::from_raw_os_error(code as i32)
    }
}

/// The Win32 code of an I/O error, from either form.
pub(super) fn code(err: &io::Error) -> Option<u32> {
    let code = err.raw_os_error()? as u32;
    Some(if code & 0xFFFF_0000 == 0x8007_0000 {
        code & 0xFFFF
    } else {
        code
    })
}

/// A security descriptor from SDDL, freed when dropped.
struct Descriptor(PSECURITY_DESCRIPTOR);

impl Descriptor {
    fn new(sddl: &str) -> io::Result<Self> {
        let mut descriptor = PSECURITY_DESCRIPTOR::default();
        unsafe {
            ConvertStringSecurityDescriptorToSecurityDescriptorW(
                &HSTRING::from(sddl),
                SDDL_REVISION_1,
                &mut descriptor,
                None,
            )
        }
        .map_err(win)?;
        Ok(Self(descriptor))
    }

    fn attributes(&self) -> SECURITY_ATTRIBUTES {
        SECURITY_ATTRIBUTES {
            nLength: size_of::<SECURITY_ATTRIBUTES>() as u32,
            lpSecurityDescriptor: self.0.0,
            bInheritHandle: false.into(),
        }
    }
}

impl Drop for Descriptor {
    fn drop(&mut self) {
        unsafe { LocalFree(Some(HLOCAL(self.0.0))) };
    }
}
