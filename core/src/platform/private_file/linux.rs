//! Linux: file systems known to enforce permissions locally, and POSIX ACLs removed and checked
//! through their extended attributes.

use std::{
    ffi::CStr,
    io,
    os::fd::{AsRawFd, BorrowedFd},
    path::Path,
};

use super::{PrivateFileError, linux_enforces_permissions};

const ACCESS_ACL: &CStr = c"system.posix_acl_access";
const DEFAULT_ACL: &CStr = c"system.posix_acl_default";

pub(super) fn check_file_system(
    folder: BorrowedFd,
    location: &Path,
) -> Result<(), PrivateFileError> {
    if linux_enforces_permissions(fs_type(folder.as_raw_fd())?) {
        Ok(())
    } else {
        Err(PrivateFileError::Unprotectable {
            location: location.to_path_buf(),
        })
    }
}

fn fs_type(fd: std::os::fd::RawFd) -> io::Result<u32> {
    let mut stat = std::mem::MaybeUninit::<libc::statfs>::uninit();
    if unsafe { libc::fstatfs(fd, stat.as_mut_ptr()) } != 0 {
        return Err(io::Error::last_os_error());
    }
    // The magic numbers fit in 32 bits; `f_type`'s width differs between architectures.
    Ok(unsafe { stat.assume_init() }.f_type as u32)
}

/// Removes the ACLs a parent's default ACL gave the folder.
pub(super) fn clear_acl(fd: BorrowedFd) -> io::Result<()> {
    for name in [ACCESS_ACL, DEFAULT_ACL] {
        if unsafe { libc::fremovexattr(fd.as_raw_fd(), name.as_ptr()) } != 0 {
            let err = io::Error::last_os_error();
            if !absent(&err) {
                return Err(err);
            }
        }
    }
    Ok(())
}

pub(super) fn has_acl(fd: BorrowedFd) -> io::Result<bool> {
    for name in [ACCESS_ACL, DEFAULT_ACL] {
        let size =
            unsafe { libc::fgetxattr(fd.as_raw_fd(), name.as_ptr(), std::ptr::null_mut(), 0) };
        if size >= 0 {
            return Ok(true);
        }
        let err = io::Error::last_os_error();
        if !absent(&err) {
            return Err(err);
        }
    }
    Ok(false)
}

/// With a POSIX ACL the group bits of the mode are its mask, which bounds every named user and
/// group, so the mode check already covers them.
pub(super) fn acl_grants_others_write(_folder: BorrowedFd) -> io::Result<bool> {
    Ok(false)
}

/// As for writing: the mode's group bits bound every ACL entry.
pub(super) fn acl_grants_read(_path: &Path) -> io::Result<bool> {
    Ok(false)
}

/// No such attribute, or a file system without them, where the mode is authoritative.
fn absent(err: &io::Error) -> bool {
    // `ENOTSUP` is `EOPNOTSUPP` on Linux.
    matches!(err.raw_os_error(), Some(libc::ENODATA | libc::ENOTSUP))
}
