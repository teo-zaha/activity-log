//! A child and what it starts, as one job object.

use std::{
    os::windows::{io::AsRawHandle, process::CommandExt},
    process::{Child, Command},
};

use windows::{
    Win32::{
        Foundation::{CloseHandle, HANDLE},
        System::{
            Diagnostics::ToolHelp::{
                CreateToolhelp32Snapshot, TH32CS_SNAPTHREAD, THREADENTRY32, Thread32First,
                Thread32Next,
            },
            JobObjects::{
                AssignProcessToJobObject, CreateJobObjectW, JOB_OBJECT_LIMIT_KILL_ON_JOB_CLOSE,
                JOBOBJECT_EXTENDED_LIMIT_INFORMATION, JobObjectExtendedLimitInformation,
                SetInformationJobObject, TerminateJobObject,
            },
            Threading::{
                CREATE_NO_WINDOW, CREATE_SUSPENDED, OpenThread, ResumeThread, THREAD_SUSPEND_RESUME,
            },
        },
    },
    core::PCWSTR,
};

pub(super) struct Tree(HANDLE);

/// Starts the child suspended, so it starts nothing before it is in the job, and without a
/// console window (the app has none to share).
pub(super) fn prepare(command: &mut Command) {
    command.creation_flags((CREATE_SUSPENDED | CREATE_NO_WINDOW).0);
}

/// Puts the suspended child in a job whose processes, the ones it starts included, end with
/// the job, then lets it run.
pub(super) fn adopt(child: &Child) -> std::io::Result<Tree> {
    let tree = into_job(child)?;
    resume(child.id())?;
    Ok(tree)
}

fn into_job(child: &Child) -> std::io::Result<Tree> {
    let failed = |err: windows::core::Error| std::io::Error::other(err.to_string());
    unsafe {
        let tree = Tree(CreateJobObjectW(None, PCWSTR::null()).map_err(failed)?);
        let mut limits = JOBOBJECT_EXTENDED_LIMIT_INFORMATION::default();
        limits.BasicLimitInformation.LimitFlags = JOB_OBJECT_LIMIT_KILL_ON_JOB_CLOSE;
        SetInformationJobObject(
            tree.0,
            JobObjectExtendedLimitInformation,
            (&raw const limits).cast(),
            size_of::<JOBOBJECT_EXTENDED_LIMIT_INFORMATION>() as u32,
        )
        .map_err(failed)?;
        AssignProcessToJobObject(tree.0, HANDLE(child.as_raw_handle())).map_err(failed)?;
        Ok(tree)
    }
}

/// Resumes the only thread of the suspended process `pid`, found in a snapshot of the threads.
/// A failure leaves the process suspended, and `run_cancellable` kills it.
fn resume(pid: u32) -> std::io::Result<()> {
    let failed = |err: windows::core::Error| std::io::Error::other(err.to_string());
    unsafe {
        let snapshot = CreateToolhelp32Snapshot(TH32CS_SNAPTHREAD, 0).map_err(failed)?;
        let mut entry = THREADENTRY32 {
            dwSize: size_of::<THREADENTRY32>() as u32,
            ..Default::default()
        };
        let mut found = Thread32First(snapshot, &mut entry).map_err(failed);
        let mut resumed = Err(std::io::Error::other("the process has no thread"));
        while found.is_ok() {
            if entry.th32OwnerProcessID == pid {
                resumed = OpenThread(THREAD_SUSPEND_RESUME, false, entry.th32ThreadID)
                    .map_err(failed)
                    .and_then(|thread| {
                        let suspended = ResumeThread(thread);
                        let error = std::io::Error::last_os_error();
                        let _ = CloseHandle(thread);
                        // Created suspended once, so it was suspended exactly once: anything
                        // else is not the thread this started.
                        match suspended {
                            1 => Ok(()),
                            u32::MAX => Err(error),
                            other => Err(std::io::Error::other(format!(
                                "the started thread was suspended {other} times, not once"
                            ))),
                        }
                    });
                break;
            }
            found = Thread32Next(snapshot, &mut entry).map_err(failed);
        }
        let _ = CloseHandle(snapshot);
        resumed
    }
}

impl Tree {
    /// Kills every process left in the job.
    pub(super) fn kill(&self) {
        unsafe {
            let _ = TerminateJobObject(self.0, 1);
        }
    }
}

impl Drop for Tree {
    fn drop(&mut self) {
        unsafe {
            let _ = CloseHandle(self.0);
        }
    }
}
