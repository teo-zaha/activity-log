//! macOS: APFS/HFS+ volumes that keep ownership, and extended ACLs cleared and checked through
//! libSystem's ACL functions.

use std::{
    ffi::{CStr, CString, c_char, c_int, c_uint, c_void},
    io,
    os::{
        fd::{AsRawFd, BorrowedFd},
        unix::ffi::OsStrExt,
    },
    path::Path,
};

use super::{PrivateFileError, macos_enforces_permissions};

type Acl = *mut c_void;
type AclEntry = *mut c_void;

const ACL_TYPE_EXTENDED: c_uint = 0x0000_0100;
const ACL_FIRST_ENTRY: c_int = 0;
const ACL_NEXT_ENTRY: c_int = -1;
const ACL_EXTENDED_ALLOW: c_int = 1;
const ACL_READ_DATA: u64 = 1 << 1;
const ACL_WRITE_DATA: u64 = 1 << 2; // ACL_ADD_FILE on folders
const ACL_DELETE: u64 = 1 << 4;
const ACL_APPEND_DATA: u64 = 1 << 5; // ACL_ADD_SUBDIRECTORY on folders
const ACL_DELETE_CHILD: u64 = 1 << 6;
const ACL_WRITE_SECURITY: u64 = 1 << 12;
const ACL_CHANGE_OWNER: u64 = 1 << 13;

unsafe extern "C" {
    fn acl_init(count: c_int) -> Acl;
    fn acl_free(obj: *mut c_void) -> c_int;
    fn acl_get_fd(fd: c_int) -> Acl;
    fn acl_set_fd(fd: c_int, acl: Acl) -> c_int;
    fn acl_get_file(path: *const c_char, kind: c_uint) -> Acl;
    fn acl_get_entry(acl: Acl, entry_id: c_int, entry: *mut AclEntry) -> c_int;
    fn acl_get_tag_type(entry: AclEntry, tag: *mut c_int) -> c_int;
    fn acl_get_permset_mask_np(entry: AclEntry, mask: *mut u64) -> c_int;
    fn acl_get_qualifier(entry: AclEntry) -> *mut c_void;
    fn mbr_uid_to_uuid(uid: libc::uid_t, uuid: *mut u8) -> c_int;
}

/// One ACL entry: whether it allows, what, and to whom (a membership UUID).
struct Entry {
    allow: bool,
    mask: u64,
    principal: Option<[u8; 16]>,
}

#[cfg(test)]
thread_local! {
    /// Makes clearing an ACL fail, as on a volume that refuses it.
    pub(super) static FAIL_CLEAR_ACL: std::cell::Cell<bool> = const { std::cell::Cell::new(false) };
}

pub(super) fn check_file_system(
    folder: BorrowedFd,
    location: &Path,
) -> Result<(), PrivateFileError> {
    let mut stat = std::mem::MaybeUninit::<libc::statfs>::uninit();
    if unsafe { libc::fstatfs(folder.as_raw_fd(), stat.as_mut_ptr()) } != 0 {
        return Err(io::Error::last_os_error().into());
    }
    let stat = unsafe { stat.assume_init() };
    let name = unsafe { CStr::from_ptr(stat.f_fstypename.as_ptr()) }.to_string_lossy();
    let ignores_ownership = stat.f_flags & libc::MNT_IGNORE_OWNERSHIP as u32 != 0;
    if macos_enforces_permissions(&name, ignores_ownership) {
        Ok(())
    } else {
        Err(PrivateFileError::Unprotectable {
            location: location.to_path_buf(),
        })
    }
}

/// Sets an empty ACL, which removes any entry inherited from the parent folder.
pub(super) fn clear_acl(fd: BorrowedFd) -> io::Result<()> {
    #[cfg(test)]
    if FAIL_CLEAR_ACL.get() {
        return Err(io::Error::other("ACL cannot be cleared"));
    }
    let acl = unsafe { acl_init(0) };
    if acl.is_null() {
        return Err(io::Error::last_os_error());
    }
    let result = unsafe { acl_set_fd(fd.as_raw_fd(), acl) };
    let err = io::Error::last_os_error();
    unsafe { acl_free(acl) };
    if result == 0 { Ok(()) } else { Err(err) }
}

pub(super) fn has_acl(fd: BorrowedFd) -> io::Result<bool> {
    Ok(!entries(unsafe { acl_get_fd(fd.as_raw_fd()) })?.is_empty())
}

/// Whether an allow entry lets another principal create, rename or delete entries in the
/// folder, or change its protection.
pub(super) fn acl_grants_others_write(folder: BorrowedFd) -> io::Result<bool> {
    const WRITE: u64 = ACL_WRITE_DATA
        | ACL_APPEND_DATA
        | ACL_DELETE
        | ACL_DELETE_CHILD
        | ACL_WRITE_SECURITY
        | ACL_CHANGE_OWNER;
    grants_others(unsafe { acl_get_fd(folder.as_raw_fd()) }, WRITE)
}

pub(super) fn acl_grants_read(path: &Path) -> io::Result<bool> {
    let path = CString::new(path.as_os_str().as_bytes())
        .map_err(|_| io::Error::new(io::ErrorKind::InvalidInput, "a path with a NUL byte"))?;
    grants_others(
        unsafe { acl_get_file(path.as_ptr(), ACL_TYPE_EXTENDED) },
        ACL_READ_DATA,
    )
}

/// Whether an allow entry of `acl` gives anyone but the current user any of `rights`.
fn grants_others(acl: Acl, rights: u64) -> io::Result<bool> {
    let entries = entries(acl)?;
    if entries.is_empty() {
        return Ok(false);
    }
    let user = user_uuid()?;
    Ok(entries
        .iter()
        .any(|entry| entry.allow && entry.mask & rights != 0 && entry.principal != Some(user)))
}

fn user_uuid() -> io::Result<[u8; 16]> {
    let mut uuid = [0u8; 16];
    match unsafe { mbr_uid_to_uuid(libc::geteuid(), uuid.as_mut_ptr()) } {
        0 => Ok(uuid),
        code => Err(io::Error::from_raw_os_error(code)),
    }
}

/// The number of entries of an ACL, for tests.
#[cfg(test)]
pub(super) fn acl_entry_count(path: &Path) -> io::Result<usize> {
    let path = CString::new(path.as_os_str().as_bytes()).unwrap();
    Ok(entries(unsafe { acl_get_file(path.as_ptr(), ACL_TYPE_EXTENDED) })?.len())
}

/// The entries of `acl`, which is then freed; a missing ACL has none.
fn entries(acl: Acl) -> io::Result<Vec<Entry>> {
    if acl.is_null() {
        let err = io::Error::last_os_error();
        return match err.raw_os_error() {
            Some(libc::ENOENT) => Ok(Vec::new()),
            _ => Err(err),
        };
    }
    let result = read_entries(acl);
    unsafe { acl_free(acl) };
    result
}

fn read_entries(acl: Acl) -> io::Result<Vec<Entry>> {
    let mut entries = Vec::new();
    let mut entry: AclEntry = std::ptr::null_mut();
    let mut id = ACL_FIRST_ENTRY;
    while unsafe { acl_get_entry(acl, id, &mut entry) } == 0 {
        id = ACL_NEXT_ENTRY;
        let (mut tag, mut mask) = (0, 0);
        if unsafe { acl_get_tag_type(entry, &mut tag) } != 0
            || unsafe { acl_get_permset_mask_np(entry, &mut mask) } != 0
        {
            return Err(io::Error::last_os_error());
        }
        let qualifier = unsafe { acl_get_qualifier(entry) };
        let principal = (!qualifier.is_null()).then(|| {
            let mut uuid = [0u8; 16];
            unsafe {
                std::ptr::copy_nonoverlapping(qualifier.cast::<u8>(), uuid.as_mut_ptr(), 16);
                acl_free(qualifier);
            }
            uuid
        });
        entries.push(Entry {
            allow: tag == ACL_EXTENDED_ALLOW,
            mask,
            principal,
        });
    }
    Ok(entries)
}
