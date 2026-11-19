//! Single-file, per-user installer for DXF Converter (no admin rights needed).
//!
//!   dxf-converter-setup.exe               interactive install / update
//!   dxf-converter-setup.exe /quiet        silent install (for IT / scripts)
//!   uninstall.exe /uninstall [/quiet]     remove (used by Windows "Apps & features")
//!
//! Installs to %LOCALAPPDATA%\Programs\DXF Converter, adds Desktop and Start-menu
//! shortcuts and registers an uninstall entry under HKCU.

#![windows_subsystem = "windows"]

use rfd::{MessageButtons, MessageDialog, MessageDialogResult, MessageLevel};
use std::os::windows::process::CommandExt;
use std::path::{Path, PathBuf};
use std::process::Command;

const APP_EXE: &[u8] = include_bytes!(env!("DXF_APP_EXE", "set DXF_APP_EXE (use package.ps1)"));
const VERSION: &str = env!("DXF_APP_VERSION", "set DXF_APP_VERSION (use package.ps1)");
const APP_NAME: &str = "DXF Converter";
const PUBLISHER: &str = "NHL";
const EXE_NAME: &str = "DXF Converter.exe";
const UNINSTALLER: &str = "uninstall.exe";
const REG_KEY: &str = r"HKCU\Software\Microsoft\Windows\CurrentVersion\Uninstall\NHL-DXF-Converter";
const CREATE_NO_WINDOW: u32 = 0x0800_0000;
const DETACHED_PROCESS: u32 = 0x0000_0008;

fn main() {
    let args: Vec<String> = std::env::args().skip(1).map(|a| a.to_ascii_lowercase()).collect();
    let quiet = args.iter().any(|a| matches!(a.as_str(), "/quiet" | "/s" | "/silent" | "--quiet"));
    let remove = args.iter().any(|a| matches!(a.as_str(), "/uninstall" | "--uninstall" | "/u"));
    let result = if remove { uninstall(quiet) } else { install(quiet) };
    if let Err(msg) = result {
        if !quiet {
            message(MessageLevel::Error, &format!("{APP_NAME} Setup"), &msg);
        }
        std::process::exit(1);
    }
}

// ---------------------------------------------------------------- helpers

fn install_dir() -> Result<PathBuf, String> {
    let base = std::env::var_os("LOCALAPPDATA").ok_or("LOCALAPPDATA is not set")?;
    let base = PathBuf::from(base);
    if !base.is_absolute() {
        return Err("LOCALAPPDATA is not an absolute path".into());
    }
    Ok(base.join("Programs").join(APP_NAME))
}

/// Full path of a Windows system tool. Never rely on the search path: Windows looks in the
/// folder of the running exe first, which for a downloaded setup is the Downloads folder.
fn system_tool(rel: &str) -> PathBuf {
    let root = std::env::var_os("SystemRoot")
        .or_else(|| std::env::var_os("windir"))
        .map(PathBuf::from)
        .filter(|p| p.is_absolute())
        .unwrap_or_else(|| PathBuf::from(r"C:\Windows"));
    root.join("System32").join(rel)
}

fn message(level: MessageLevel, title: &str, text: &str) {
    MessageDialog::new().set_level(level).set_title(title).set_description(text).set_buttons(MessageButtons::Ok).show();
}

fn ask(level: MessageLevel, title: &str, text: &str) -> bool {
    matches!(
        MessageDialog::new()
            .set_level(level)
            .set_title(title)
            .set_description(text)
            .set_buttons(MessageButtons::YesNo)
            .show(),
        MessageDialogResult::Yes
    )
}

/// Run a system tool without a console window and return its stdout, or an error with its stderr.
fn run_tool(what: &str, cmd: &mut Command) -> Result<String, String> {
    let out = cmd
        .creation_flags(CREATE_NO_WINDOW)
        .output()
        .map_err(|e| format!("Could not start {what}: {e}"))?;
    if out.status.success() {
        Ok(String::from_utf8_lossy(&out.stdout).into_owned())
    } else {
        Err(format!("{what} failed: {}", String::from_utf8_lossy(&out.stderr).trim()))
    }
}

/// Run a PowerShell snippet; values are passed through environment variables (no quoting issues).
fn powershell(script: &str, env: &[(&str, &str)]) -> Result<(), String> {
    let mut cmd = Command::new(system_tool(r"WindowsPowerShell\v1.0\powershell.exe"));
    cmd.args(["-NoProfile", "-NonInteractive", "-ExecutionPolicy", "Bypass", "-Command", script]);
    for (k, v) in env {
        cmd.env(k, v);
    }
    run_tool("PowerShell", &mut cmd).map(|_| ())
}

fn reg(args: &[&str]) -> Result<String, String> {
    run_tool("Registry update", Command::new(system_tool("reg.exe")).args(args))
}

fn reg_sz(name: &str, value: &str) -> Result<(), String> {
    reg(&["add", REG_KEY, "/v", name, "/t", "REG_SZ", "/d", value, "/f"]).map(|_| ())
}

fn reg_dword(name: &str, value: u32) -> Result<(), String> {
    reg(&["add", REG_KEY, "/v", name, "/t", "REG_DWORD", "/d", &value.to_string(), "/f"]).map(|_| ())
}

/// Version recorded by a previous install, if any.
fn installed_version() -> Option<String> {
    let out = reg(&["query", REG_KEY, "/v", "DisplayVersion"]).ok()?;
    out.lines()
        .find(|l| l.contains("DisplayVersion"))
        .and_then(|l| l.split_whitespace().last())
        .map(str::to_string)
}

/// "1.2.10" -> [1, 2, 10]; missing or non-numeric parts count as 0.
fn version_key(v: &str) -> Vec<u64> {
    v.split('.').map(|p| p.trim().parse().unwrap_or(0)).collect()
}

/// Is the installed app currently running (for this user)?
fn app_running() -> bool {
    let filter = format!("IMAGENAME eq {EXE_NAME}");
    match run_tool("tasklist", Command::new(system_tool("tasklist.exe")).args(["/FI", &filter, "/NH", "/FO", "CSV"])) {
        Ok(out) => out.to_ascii_lowercase().contains(&EXE_NAME.to_ascii_lowercase()),
        Err(_) => false,
    }
}

fn close_app_message() -> String {
    format!("{APP_NAME} is currently open.\n\nPlease close it and try again.")
}

const SHORTCUT_FOLDERS: &str = "@([Environment]::GetFolderPath('Desktop'), [Environment]::GetFolderPath('Programs'))";

fn shortcut_script() -> String {
    format!(
        r#"
$ErrorActionPreference = 'Stop'
$ws = New-Object -ComObject WScript.Shell
foreach ($folder in {SHORTCUT_FOLDERS}) {{
    $lnk = $ws.CreateShortcut((Join-Path $folder ($env:DXF_NAME + '.lnk')))
    $lnk.TargetPath = $env:DXF_EXE
    $lnk.WorkingDirectory = $env:DXF_DIR
    $lnk.IconLocation = $env:DXF_EXE + ',0'
    $lnk.Description = 'Clean up DXF files for cutting'
    $lnk.Save()
}}
"#
    )
}

fn remove_shortcuts_script() -> String {
    format!(
        r#"
foreach ($folder in {SHORTCUT_FOLDERS}) {{
    Remove-Item -LiteralPath (Join-Path $folder ($env:DXF_NAME + '.lnk')) -ErrorAction SilentlyContinue
}}
"#
    )
}

/// Write the app next to its final name, then swap it in, so a failed write never
/// leaves a half-written exe behind.
fn write_app(path: &Path) -> Result<(), String> {
    let tmp = path.with_extension("exe.new");
    std::fs::write(&tmp, APP_EXE).map_err(|e| format!("Could not write {}: {e}", tmp.display()))?;
    std::fs::rename(&tmp, path).map_err(|e| {
        let _ = std::fs::remove_file(&tmp);
        format!("Could not replace {}: {e}", path.display())
    })
}

// ---------------------------------------------------------------- install

fn install(quiet: bool) -> Result<(), String> {
    let dir = install_dir()?;
    let exe = dir.join(EXE_NAME);
    let title = format!("{APP_NAME} {VERSION} Setup");

    let previous = if exe.exists() { installed_version() } else { None };
    let action = match &previous {
        None if exe.exists() => "Reinstall",
        None => "Install",
        Some(old) if version_key(old) < version_key(VERSION) => "Update",
        Some(old) if version_key(old) == version_key(VERSION) => "Reinstall",
        Some(old) => {
            // Never silently replace a newer version with an older one.
            let msg = format!("A newer version of {APP_NAME} ({old}) is already installed.\n\nThis setup contains version {VERSION}.");
            if quiet {
                return Err(msg);
            }
            if !ask(MessageLevel::Warning, &title, &format!("{msg}\n\nReplace it with the older version {VERSION}?")) {
                return Ok(());
            }
            "Downgrade"
        }
    };
    if !quiet && action != "Downgrade" {
        let text = format!(
            "{action} {APP_NAME} {VERSION}?\n\nIt is installed for your Windows account only (no admin rights needed) in:\n{}\n\nShortcuts are added to the Desktop and Start menu.",
            dir.display()
        );
        if !ask(MessageLevel::Info, &title, &text) {
            return Ok(());
        }
    }
    if app_running() {
        return Err(close_app_message());
    }

    std::fs::create_dir_all(&dir).map_err(|e| format!("Could not create {}: {e}", dir.display()))?;
    write_app(&exe)?;

    // Keep a copy of this setup program as the uninstaller.
    let me = std::env::current_exe().map_err(|e| e.to_string())?;
    let uninstaller = dir.join(UNINSTALLER);
    let same = std::fs::canonicalize(&me).ok() == std::fs::canonicalize(&uninstaller).ok();
    if !same {
        std::fs::copy(&me, &uninstaller).map_err(|e| format!("Could not write the uninstaller: {e}"))?;
    }

    // Register first, so the install can always be removed from "Apps & features"
    // even if the shortcut step fails (e.g. on a locked-down PC).
    let exe_s = exe.to_string_lossy().to_string();
    let dir_s = dir.to_string_lossy().to_string();
    let un = uninstaller.to_string_lossy();
    reg_sz("DisplayName", APP_NAME)?;
    reg_sz("DisplayVersion", VERSION)?;
    reg_sz("Publisher", PUBLISHER)?;
    reg_sz("DisplayIcon", &format!("{exe_s},0"))?;
    reg_sz("InstallLocation", &dir_s)?;
    reg_sz("UninstallString", &format!("\"{un}\" /uninstall"))?;
    reg_sz("QuietUninstallString", &format!("\"{un}\" /uninstall /quiet"))?;
    reg_dword("NoModify", 1)?;
    reg_dword("NoRepair", 1)?;
    reg_dword("EstimatedSize", ((APP_EXE.len() * 2) / 1024) as u32)?;

    let shortcuts = powershell(&shortcut_script(), &[("DXF_EXE", &exe_s), ("DXF_DIR", &dir_s), ("DXF_NAME", APP_NAME)]);

    if !quiet {
        let done = match &shortcuts {
            Ok(()) => format!("{APP_NAME} is installed.\n\nOpen it now?"),
            Err(e) => format!(
                "{APP_NAME} is installed, but the shortcuts could not be created:\n{e}\n\nYou can start it from:\n{exe_s}\n\nOpen it now?"
            ),
        };
        if ask(MessageLevel::Info, &title, &done) {
            let _ = Command::new(&exe).current_dir(&dir).spawn();
        }
    }
    shortcuts
}

// ---------------------------------------------------------------- uninstall

fn uninstall(quiet: bool) -> Result<(), String> {
    let dir = install_dir()?;
    let title = format!("{APP_NAME} Setup");
    if !quiet && !ask(MessageLevel::Info, &title, &format!("Remove {APP_NAME} from this computer?")) {
        return Ok(());
    }
    if app_running() {
        return Err(close_app_message());
    }
    let exe = dir.join(EXE_NAME);
    if exe.exists() {
        std::fs::remove_file(&exe).map_err(|e| format!("Could not remove {}: {e}", exe.display()))?;
    }
    powershell(&remove_shortcuts_script(), &[("DXF_NAME", APP_NAME)])?;
    let _ = reg(&["delete", REG_KEY, "/f"]);

    // Tell the user first: this program may live inside the install folder and keeps
    // its own file locked until it exits.
    if !quiet {
        message(MessageLevel::Info, &title, &format!("{APP_NAME} has been removed."));
    }
    if dir.exists() {
        schedule_folder_removal(&dir);
    }
    Ok(())
}

/// The cleanup script: retries for up to ~60 s until this process has exited and the
/// folder (including uninstall.exe) can be deleted, then deletes itself.
fn cleanup_script(dir: &Path, ping: &Path) -> String {
    let d = dir.display();
    let ping = ping.display();
    [
        "@echo off".to_string(),
        "for /l %%i in (1,1,30) do (".to_string(),
        format!("  \"{ping}\" -n 3 127.0.0.1 >nul"),
        format!("  rmdir /s /q \"{d}\" 2>nul"),
        format!("  if not exist \"{d}\" goto done"),
        ")".to_string(),
        ":done".to_string(),
        // delete this script without cmd complaining that the running batch file vanished
        "(goto) 2>nul & del \"%~f0\"".to_string(),
        String::new(),
    ]
    .join("\r\n")
}

/// This program may live inside the install folder and keeps its own file locked until it
/// exits, so a detached batch script removes the folder after we are gone.
fn schedule_folder_removal(dir: &Path) {
    let script = std::env::temp_dir().join(format!("dxf-converter-uninstall-{}.cmd", std::process::id()));
    if std::fs::write(&script, cleanup_script(dir, &system_tool("PING.EXE"))).is_err() {
        return;
    }
    let _ = Command::new(system_tool("cmd.exe"))
        .raw_arg(format!("/d /c \"{}\"", script.display()))
        .current_dir(std::env::temp_dir())
        .creation_flags(CREATE_NO_WINDOW | DETACHED_PROCESS)
        .spawn();
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn cleanup_script_removes_folder_and_itself() {
        let base = std::env::temp_dir().join(format!("dxf cleanup test {}", std::process::id()));
        let dir = base.join("DXF Converter");
        std::fs::create_dir_all(&dir).unwrap();
        std::fs::write(dir.join("uninstall.exe"), b"x").unwrap();
        let script = base.join("cleanup.cmd");
        std::fs::write(&script, cleanup_script(&dir, &system_tool("PING.EXE"))).unwrap();
        let status = Command::new(system_tool("cmd.exe"))
            .raw_arg(format!("/d /c \"{}\"", script.display()))
            .status()
            .unwrap();
        assert!(status.success());
        assert!(!dir.exists(), "install folder should be gone");
        assert!(!script.exists(), "script should delete itself");
        let _ = std::fs::remove_dir_all(&base);
    }

    #[test]
    fn versions_compare_numerically() {
        assert!(version_key("0.1.10") > version_key("0.1.9"));
        assert!(version_key("0.2") > version_key("0.1.9"));
        assert_eq!(version_key("0.1.1"), version_key("0.1.1"));
    }
}
