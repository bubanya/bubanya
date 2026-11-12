use std::process::Command;

/// Не показывать консольное окно у дочерних процессов на Windows.
#[cfg(target_os = "windows")]
pub fn hide_console(command: &mut Command) {
    use std::os::windows::process::CommandExt;
    const CREATE_NO_WINDOW: u32 = 0x0800_0000;
    command.creation_flags(CREATE_NO_WINDOW);
}

#[cfg(not(target_os = "windows"))]
pub fn hide_console(_command: &mut Command) {}

/// На Windows для игры лучше запускать javaw.exe, иначе открывается консоль.
#[cfg(target_os = "windows")]
pub fn launch_java(path: &str) -> String {
    let candidate = std::path::Path::new(path).with_file_name("javaw.exe");
    if candidate.is_file() {
        candidate.to_string_lossy().to_string()
    } else {
        path.to_string()
    }
}

#[cfg(not(target_os = "windows"))]
pub fn launch_java(path: &str) -> String {
    path.to_string()
}
