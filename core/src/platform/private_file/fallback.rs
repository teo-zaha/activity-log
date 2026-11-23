//! Systems without a recipe: nothing can be kept private, so nothing secret is written.

use std::{
    fs::{self, File, OpenOptions},
    io,
    path::{Path, PathBuf},
};

use super::PrivateFileError;

pub(super) struct Folder(PathBuf);

pub(super) fn open_folder(path: &Path) -> io::Result<Folder> {
    Ok(Folder(fs::canonicalize(path)?))
}

impl Folder {
    pub(super) fn open_lock(&self, name: &str) -> io::Result<File> {
        OpenOptions::new()
            .read(true)
            .write(true)
            .create(true)
            .truncate(false)
            .open(self.0.join(name))
    }

    pub(super) fn read(&self, name: &str) -> io::Result<Option<Vec<u8>>> {
        match fs::read(self.0.join(name)) {
            Ok(bytes) => Ok(Some(bytes)),
            Err(err) if err.kind() == io::ErrorKind::NotFound => Ok(None),
            Err(err) => Err(err),
        }
    }

    /// Nothing here can be kept private, so a file that exists is exposed.
    pub(super) fn read_owner_only(&self, name: &str) -> io::Result<Option<super::OwnerOnly>> {
        Ok(self.read(name)?.map(|_| super::OwnerOnly::Exposed))
    }

    pub(super) fn check_private(&self) -> Result<(), PrivateFileError> {
        Err(PrivateFileError::Unprotectable {
            location: self.0.clone(),
        })
    }

    pub(super) fn staging(&self, _target_name: &str) -> Result<Staging, PrivateFileError> {
        Err(PrivateFileError::Unprotectable {
            location: self.0.clone(),
        })
    }
}

pub(super) struct Staging(PathBuf);

impl Staging {
    pub(super) fn create(&self, _name: &str) -> io::Result<File> {
        Err(io::ErrorKind::Unsupported.into())
    }

    pub(super) fn open(&self, _name: &str) -> io::Result<File> {
        Err(io::ErrorKind::Unsupported.into())
    }

    /// Nothing holds a created file here.
    pub(super) fn release(&self, _name: &str) {}

    pub(super) fn publish(&self, _name: &str) -> io::Result<()> {
        Err(io::ErrorKind::Unsupported.into())
    }

    pub(super) fn path(&self) -> &Path {
        &self.0
    }

    pub(super) fn keep(self) -> PathBuf {
        self.0
    }
}

pub(super) fn readable_by_others(_path: &Path) -> io::Result<bool> {
    Ok(true)
}

pub(super) fn private_command(program: &std::ffi::OsStr) -> std::process::Command {
    std::process::Command::new(program)
}

pub(super) fn create_private_folder(folder: &Path) -> io::Result<()> {
    fs::create_dir_all(folder)
}
