//! Files only the current user can open, for settings and collected data.
//!
//! Secrets are written only into a file created inside a verified staging folder beside the
//! target, so no other account can ever have opened it, and the file is then moved over the
//! target through handles to the two folders, never through a path another account could
//! rename. Locations that cannot keep files private (FAT, exFAT, network shares, a folder other
//! accounts can write to) are refused before anything is created.

use std::{
    fs::File,
    io::{self, Write},
    path::{Path, PathBuf},
};

#[cfg(test)]
mod tests;

#[cfg(not(any(target_os = "macos", target_os = "linux", target_os = "windows")))]
mod fallback;
#[cfg(target_os = "linux")]
mod linux;
#[cfg(target_os = "macos")]
mod macos;
#[cfg(any(target_os = "macos", target_os = "linux"))]
mod unix;
#[cfg(target_os = "windows")]
mod windows;

#[cfg(not(any(target_os = "macos", target_os = "linux", target_os = "windows")))]
use fallback as imp;
#[cfg(any(target_os = "macos", target_os = "linux"))]
use unix as imp;
#[cfg(target_os = "windows")]
use windows as imp;

#[cfg(all(test, target_os = "macos"))]
pub(crate) use macos::acl_entry_count;

#[derive(Debug)]
pub enum PrivateFileError {
    /// The location cannot hold private files: a file system without permissions or ACLs that
    /// can be set (FAT, exFAT, some network shares), a folder other accounts can change, or
    /// protection that did not stick.
    Unprotectable {
        location: PathBuf,
    },
    Io(io::Error),
}

impl std::fmt::Display for PrivateFileError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Unprotectable { location } => unprotectable(f, location),
            Self::Io(err) => write!(f, "{err}"),
        }
    }
}

impl std::error::Error for PrivateFileError {}

fn unprotectable(f: &mut std::fmt::Formatter<'_>, location: &Path) -> std::fmt::Result {
    write!(f, "{} cannot keep files private", location.display())
}

/// Why [`Folder::replace`] did not replace the file; the file is as it was.
#[derive(Debug)]
pub enum ReplaceError {
    /// As [`PrivateFileError::Unprotectable`]: nothing was written.
    Unprotectable { location: PathBuf },
    Failed {
        step: ReplaceStep,
        source: io::Error,
    },
}

impl std::fmt::Display for ReplaceError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Unprotectable { location } => unprotectable(f, location),
            // The cause follows as the error's source.
            Self::Failed { step, .. } => write!(f, "{step}"),
        }
    }
}

impl std::error::Error for ReplaceError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            Self::Failed { source, .. } => Some(source),
            Self::Unprotectable { .. } => None,
        }
    }
}

/// The step of [`Folder::replace`] that failed.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ReplaceStep {
    /// Checking the file's name and that its folder can keep it private (the folder's owner,
    /// protection and file system).
    Checking,
    /// Creating the private staging folder beside the file.
    Staging,
    /// Creating the new file inside the staging folder.
    Creating,
    Writing,
    Flushing,
    /// Moving the new file over the old one.
    Publishing,
}

impl std::fmt::Display for ReplaceStep {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(match self {
            Self::Checking => "checking that its folder can keep it private",
            Self::Staging => "creating a private folder beside it",
            Self::Creating => "creating the new file",
            Self::Writing => "writing the new file",
            Self::Flushing => "flushing the new file to disk",
            Self::Publishing => "moving the new file over it",
        })
    }
}

impl From<io::Error> for PrivateFileError {
    fn from(err: io::Error) -> Self {
        Self::Io(err)
    }
}

/// What [`Folder::read_owner_only`] found.
pub enum OwnerOnly {
    /// Only the user can reach it; its bytes, read into a buffer sized for them.
    Private(Vec<u8>),
    /// Another account can read or change it, or it is not a regular file of the user's:
    /// nothing was read.
    Exposed,
}

/// A folder held open, so that locking, reading and publishing a file in it all happen in the
/// same folder even if its path is moved or relinked meanwhile.
pub struct Folder(imp::Folder);

/// Opens the folder at `path` (links in the path resolved once).
pub fn open_folder(path: &Path) -> io::Result<Folder> {
    imp::open_folder(path).map(Folder)
}

impl Folder {
    /// Opens or creates `name` inside, for locking; never through a link. It holds nothing, so
    /// it needs no protection.
    pub fn open_lock(&self, name: &str) -> io::Result<File> {
        check_name(name)?;
        self.0.open_lock(name)
    }

    /// The bytes of `name` inside, `None` when it does not exist; never through a link.
    pub fn read(&self, name: &str) -> io::Result<Option<Vec<u8>>> {
        check_name(name)?;
        self.0.read(name)
    }

    /// The bytes of `name` inside if only the user can reach it, judged on the very handle
    /// they are read through: a regular file owned by the user that no other account may
    /// read or write (no group or other mode bits, no extended ACL entry on macOS and Linux;
    /// no rights for another account on Windows). `None` when it does not exist.
    pub fn read_owner_only(&self, name: &str) -> io::Result<Option<OwnerOnly>> {
        check_name(name)?;
        self.0.read_owner_only(name)
    }

    /// Whether this folder can keep files private: owned by the user, no other account may
    /// add, rename or delete entries, and a file system that enforces it. Checked again by
    /// [`Self::staging`]; a reader of private data checks it before trusting what it reads.
    pub fn check_private(&self) -> Result<(), PrivateFileError> {
        self.0.check_private()
    }

    /// A staging folder in this folder for replacing `target_name`, after checking that this
    /// folder can keep files private: owned by the user, no other account may add, rename or
    /// delete entries, and a file system that enforces it.
    pub fn staging(&self, target_name: &str) -> Result<Staging, PrivateFileError> {
        check_name(target_name)?;
        self.0.staging(target_name).map(Staging)
    }

    /// Replaces `name` inside with a new owner-only file holding `bytes`: written into a
    /// staging folder beside it ([`Self::staging`]), flushed to disk, and moved over it in one
    /// step. A failure leaves the old file as it was and names its step
    /// ([`ReplaceError::Failed`]); a folder that cannot keep files private is
    /// [`ReplaceError::Unprotectable`], with nothing written.
    pub fn replace(&self, name: &str, bytes: &[u8]) -> Result<(), ReplaceError> {
        let failed = |step| move |source| ReplaceError::Failed { step, source };
        let refused = |step| {
            move |err| match err {
                PrivateFileError::Unprotectable { location } => {
                    ReplaceError::Unprotectable { location }
                }
                PrivateFileError::Io(source) => ReplaceError::Failed { step, source },
            }
        };
        check_name(name).map_err(failed(ReplaceStep::Checking))?;
        self.check_private()
            .map_err(refused(ReplaceStep::Checking))?;
        // Checks the folder once more as it creates the staging folder inside.
        let staging = self.staging(name).map_err(refused(ReplaceStep::Staging))?;
        let new = format!("{name}.new");
        let mut file = staging
            .create(&new)
            .map_err(failed(ReplaceStep::Creating))?;
        file.write_all(bytes)
            .map_err(failed(ReplaceStep::Writing))?;
        file.sync_all().map_err(failed(ReplaceStep::Flushing))?;
        drop(file);
        staging
            .publish(&new)
            .map_err(failed(ReplaceStep::Publishing))
    }
}

/// A folder beside a target file in which only the current user can open or create anything.
/// Removed with everything in it when dropped, unless [`Staging::keep`] is called.
pub struct Staging(imp::Staging);

/// A folder beside `target` (same file system) in which only the current user can open or
/// create anything, verified before it is returned. Every later operation goes through handles
/// to it and to the target's folder, never through a path.
#[allow(
    dead_code,
    reason = "writers stage through the folder they locked and read with (`Folder::staging`)"
)]
pub fn private_staging(target: &Path) -> Result<Staging, PrivateFileError> {
    let name = target
        .file_name()
        .and_then(|name| name.to_str())
        .ok_or_else(|| io::Error::new(io::ErrorKind::InvalidInput, "not a file path"))?;
    let parent = match target.parent() {
        Some(parent) if !parent.as_os_str().is_empty() => parent,
        _ => Path::new("."),
    };
    open_folder(parent)?.staging(name)
}

impl Staging {
    /// A new file inside, private from its creation (fails if the name exists).
    pub fn create(&self, name: &str) -> io::Result<File> {
        check_name(name)?;
        self.0.create(name)
    }

    /// Opens a file inside by name for reading, without following links.
    pub fn open(&self, name: &str) -> io::Result<File> {
        check_name(name)?;
        self.0.open(name)
    }

    /// Moves a file created in this folder over the target atomically, through the handle it
    /// was written with where the system needs one. The result keeps its private protection.
    pub fn publish(&self, name: &str) -> io::Result<()> {
        check_name(name)?;
        self.0.publish(name)
    }

    /// Lets another program write and replace `name` (`config edit`'s editor): on Windows the
    /// handle it was created through, which keeps others out, is closed. It can then no longer
    /// be published.
    pub fn release(&self, name: &str) {
        self.0.release(name);
    }

    /// For handing a file inside to another program (`config edit`'s editor).
    pub fn path(&self) -> &Path {
        self.0.path()
    }

    /// Leaves the folder and its files in place, for the user to pick up; returns its path.
    pub fn keep(self) -> PathBuf {
        self.0.keep()
    }
}

/// A name is one ordinary entry of a folder: no separator, no `.` or `..`, and on Windows one
/// that Win32 and NT read alike ([`windows_reads_alike`]).
fn check_name(name: &str) -> io::Result<()> {
    let single = !name.is_empty()
        && name != "."
        && name != ".."
        && !name.contains(['/', '\\', '\0'])
        && (!cfg!(windows) || windows_reads_alike(name));
    if single {
        Ok(())
    } else {
        Err(io::Error::new(
            io::ErrorKind::InvalidInput,
            format!("{name:?} is not a file name"),
        ))
    }
}

/// Whether a Win32 path and the NT calls name the same file by `name`: Win32 drops a trailing
/// `.` or space and reads a reserved device name (`CON`, `NUL`, `COM1`, … also with an
/// extension) as the device, while the NT rename takes the name as written; `:` names a
/// stream.
#[cfg_attr(not(any(target_os = "windows", test)), allow(dead_code))]
fn windows_reads_alike(name: &str) -> bool {
    const DEVICES: [&str; 4] = ["CON", "PRN", "AUX", "NUL"];
    const NUMBERED: [&str; 2] = ["COM", "LPT"];
    // Microsoft's list of reserved names includes COM0 and LPT0, and the superscript digits.
    const DIGITS: [char; 13] = [
        '0', '1', '2', '3', '4', '5', '6', '7', '8', '9', '\u{b9}', '\u{b2}', '\u{b3}',
    ];
    let stem = name.split('.').next().unwrap_or(name).trim_end_matches(' ');
    let numbered = |prefix: &str| {
        let mut chars = stem.chars();
        let start: String = chars.by_ref().take(3).collect();
        start.eq_ignore_ascii_case(prefix)
            && chars.next().is_some_and(|digit| DIGITS.contains(&digit))
            && chars.next().is_none()
    };
    let device = DEVICES
        .iter()
        .any(|device| stem.eq_ignore_ascii_case(device))
        || NUMBERED.into_iter().any(numbered);
    !name.contains(':') && !name.ends_with(['.', ' ']) && !device
}

/// Whether the open `file` is owned by the current user, judged on its handle (on Windows, its
/// owner is the user's own SID). A system that cannot tell answers `false`.
pub fn owned_by_user(file: &File) -> io::Result<bool> {
    imp::owned_by_user(file)
}

/// Whether an existing file can be opened by anyone but its owner (mode bits, ACL entries).
pub fn readable_by_others(path: &Path) -> io::Result<bool> {
    imp::readable_by_others(path)
}

/// A command for another program working in a staging folder (an editor): what it creates is
/// owner-only too. On Unix its umask is `077`; on Windows files inherit the folder's ACE.
pub fn private_command(program: &std::ffi::OsStr) -> std::process::Command {
    imp::private_command(program)
}

/// Creates `folder` and its missing parents, each readable only by the current user.
pub fn create_private_folder(folder: &Path) -> io::Result<()> {
    imp::create_private_folder(folder)
}

/// Windows ACL decisions, kept apart from the API calls so they can be tested everywhere.
#[cfg_attr(not(any(target_os = "windows", test)), allow(dead_code))]
mod acl {
    pub(super) const ACCESS_ALLOWED: u8 = 0;
    /// Deny ACEs (plain, object, callback, callback object) only take rights away.
    const DENY: [u8; 4] = [1, 6, 10, 12];
    const OBJECT_INHERIT: u8 = 0x1;
    const CONTAINER_INHERIT: u8 = 0x2;
    const INHERIT_ONLY: u8 = 0x8;
    /// `FA` in SDDL.
    pub(super) const FILE_ALL_ACCESS: u32 = 0x001F_01FF;
    const SYSTEM: &str = "S-1-5-18";
    pub(super) const ADMINISTRATORS: &str = "S-1-5-32-544";

    /// One ACE as the checks read it; `sid` only for plain allow ACEs.
    pub(super) struct AceView {
        pub kind: u8,
        pub flags: u8,
        pub mask: u32,
        pub sid: Option<String>,
    }

    /// What an object created or read in a staging folder must look like.
    #[derive(Clone, Copy, PartialEq, Eq)]
    pub(super) enum Expected {
        /// The staging folder: protected, one full-access ACE for the user that every file
        /// and folder inside inherits.
        Folder,
        /// A file the app created: protected, one full-access ACE for the user.
        CreatedFile,
        /// A file an editor wrote in the folder: only allow ACEs for the user, inherited or not.
        EditorFile,
    }

    pub(super) fn trusted(principal: &str, user: &str) -> bool {
        principal == user || principal == SYSTEM || principal == ADMINISTRATORS
    }

    /// Whether an ACE may give an account other than the user, SYSTEM or Administrators any of
    /// `rights`. Allow ACEs of other forms (object, conditional callback) count as such grants,
    /// whoever they name; `None` (no DACL) grants everyone everything.
    pub(super) fn grants_others(aces: Option<&[AceView]>, user: &str, rights: u32) -> bool {
        let Some(aces) = aces else {
            return true;
        };
        aces.iter().any(|ace| {
            !DENY.contains(&ace.kind)
                && ace.flags & INHERIT_ONLY == 0
                && ace.mask & rights != 0
                && (ace.kind != ACCESS_ALLOWED
                    || !ace.sid.as_deref().is_some_and(|sid| trusted(sid, user)))
        })
    }

    /// Whether an object's owner, protection and ACEs are what `expected` asks.
    pub(super) fn owner_only(
        owner: &str,
        user: &str,
        protected: bool,
        aces: Option<&[AceView]>,
        expected: Expected,
    ) -> bool {
        let Some(aces) = aces else {
            return false;
        };
        let own = |ace: &AceView| ace.kind == ACCESS_ALLOWED && ace.sid.as_deref() == Some(user);
        let single_full = |flags: u8| {
            matches!(aces, [ace] if own(ace)
                && ace.mask == FILE_ALL_ACCESS
                && ace.flags & INHERIT_ONLY == 0
                && ace.flags & flags == flags)
        };
        (owner == user || owner == ADMINISTRATORS)
            && match expected {
                Expected::Folder => protected && single_full(OBJECT_INHERIT | CONTAINER_INHERIT),
                Expected::CreatedFile => protected && single_full(0),
                Expected::EditorFile => !aces.is_empty() && aces.iter().all(own),
            }
    }
}

/// What Windows cleanup found in a staging folder.
#[cfg_attr(not(any(target_os = "windows", test)), allow(dead_code))]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Found {
    /// A plain file of the folder, opened and checked through a handle.
    File,
    /// A folder, a link, or anything that could not be opened and checked.
    Other,
}

/// Whether the staging folder may be deleted: only when it holds nothing but plain files.
/// Otherwise it is kept whole, since deleting what is inside would mean following paths.
#[cfg_attr(not(any(target_os = "windows", test)), allow(dead_code))]
fn removable(found: &[Found]) -> bool {
    found.iter().all(|found| *found == Found::File)
}

/// The staging folder's name beside `target_name`: hidden, and unique for this attempt.
fn staging_name(target_name: &str) -> String {
    format!(".{target_name}.{:016x}.staging", crate::utils::random_u64())
}

/// Linux file systems known to enforce permissions for other local accounts, by `statfs.f_type`.
/// FUSE (sshfs, rclone), NFS, CIFS/SMB, 9p, vboxsf and virtiofs can report owner-only modes
/// without enforcing them, so they are not on the list. Neither is overlayfs: whether its upper
/// layer is on one of these cannot be proven (mount ids are reused, and a later mount can cover
/// the configured upper path).
#[cfg_attr(not(any(target_os = "linux", test)), allow(dead_code))]
fn linux_enforces_permissions(f_type: u32) -> bool {
    matches!(
        f_type,
        0xEF53 // ext2, ext3, ext4
            | 0x5846_5342 // XFS
            | 0x9123_683E // Btrfs
            | 0xF2F5_2010 // F2FS
            | 0x2FC1_2FC1 // ZFS
            | 0xCA45_1A4E // bcachefs
            | 0x0102_1994 // tmpfs
    )
}

/// macOS volumes that keep owners and ACLs: APFS and HFS+, unless mounted with "Ignore ownership
/// on this volume". SMB, NFS, macFUSE, FAT and exFAT are refused.
#[cfg_attr(not(any(target_os = "macos", test)), allow(dead_code))]
fn macos_enforces_permissions(fs_type_name: &str, ignores_ownership: bool) -> bool {
    matches!(fs_type_name, "apfs" | "hfs") && !ignores_ownership
}
