use tokio::process::Command;

const CREATE_NO_WINDOW: u32 = 0x0800_0000;

pub fn powershell_command() -> Command {
    use std::os::windows::process::CommandExt;

    let mut command = Command::new("powershell.exe");
    command.creation_flags(CREATE_NO_WINDOW);
    command.arg("-NoProfile").arg("-NonInteractive");
    command
}
