//! A child and what it starts, as one process group.

use std::{
    os::unix::process::CommandExt,
    process::{Child, Command},
};

pub(super) struct Tree {
    group: libc::pid_t,
}

/// Starts the child as the leader of a group of its own, which its children join.
pub(super) fn prepare(command: &mut Command) {
    command.process_group(0);
}

pub(super) fn adopt(child: &Child) -> std::io::Result<Tree> {
    Ok(Tree {
        group: child.id() as libc::pid_t,
    })
}

impl Tree {
    /// Kills every process left in the group.
    pub(super) fn kill(&self) {
        unsafe {
            libc::kill(-self.group, libc::SIGKILL);
        }
    }
}
