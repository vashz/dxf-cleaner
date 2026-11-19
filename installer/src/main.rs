//! Single-file, per-user installer for DXF Converter (no admin rights needed).
//!
//!   dxf-converter-setup.exe               interactive install / upgrade
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

fn install_dir() -> Result<PathBuf, String> {
    let base = std::env::var_os("LOCALAPPDATA").ok_or("LOCALAPPDATA is not set")?;
    let base = PathBuf::from(base);
    if !base.is_absolute() {
        return Err("LOCALAPPDATA is not an absolute path".into());
    }
    Ok(base.join("Programs").join(APP_NAME))
}

fn message(level: MessageLevel, title: &str, text: &str) {
    MessageDialog::new().set_level(level).set_title(title).set_description(text).set_buttons(MessageButtons::Ok).show();
}

fn ask(title: &str, text: &str) -> bool {
    matches!(
        MessageDialog::new()
            .set_level(MessageLevel::Info)
            .set_title(title)
            .set_description(text)
            .set_buttons(MessageButtons::YesNo)
            .show(),
        MessageDialogResult::Yes
    )
}

fn hidden(cmd: &mut Command) -> &mut Command {
    cmd.creation_flags(CREATE_NO_WINDOW)
}

/// Run a PowerShell snippet; values are passed through environment variables (no quoting issues).
fn powershell(script: &str, env: &[(&str, &str)]) -> Result<(), String> {
    let mut cmd = Command::new("powershell.exe");
    cmd.args(["-NoProfile", "-NonInteractive", "-ExecutionPolicy", "Bypass", "-Command", script]);
    for (k, v) in env {
        cmd.env(k, v);
    }
    let out = hidden(&mut cmd).output().map_err(|e| format!("Could not start PowerShell: {e}"))?;
    if out.status.success() {
        Ok(())
    } else {
        Err(format!("PowerShell failed: {}", String::from_utf8_lossy(&out.stderr).trim()))
    }
}

fn reg(args: &[&str]) -> Result<(), String> {
    let out = hidden(Command::new("reg.exe").args(args)).output().map_err(|e| format!("Could not run reg.exe: {e}"))?;
    if out.status.success() {
        Ok(())
    } else {
        Err(format!("Registry update failed: {}", String::from_utf8_lossy(&out.stderr).trim()))
    }
}

fn reg_sz(name: &str, value: &str) -> Result<(), String> {
    reg(&["add", REG_KEY, "/v", name, "/t", "REG_SZ", "/d", value, "/f"])
}

fn reg_dword(name: &str, value: u32) -> Result<(), String> {
    reg(&["add", REG_KEY, "/v", name, "/t", "REG_DWORD", "/d", &value.to_string(), "/f"])
}

const SHORTCUT_SCRIPT: &str = r#"
$ErrorActionPreference = 'Stop'
$ws = New-Object -ComObject WScript.Shell
foreach ($folder in @([Environment]::GetFolderPath('Desktop'), [Environment]::GetFolderPath('Programs'))) {
    $lnk = $ws.CreateShortcut((Join-Path $folder ($env:DXF_NAME + '.lnk')))
    $lnk.TargetPath = $env:DXF_EXE
    $lnk.WorkingDirectory = $env:DXF_DIR
    $lnk.IconLocation = $env:DXF_EXE + ',0'
    $lnk.Description = 'Clean up DXF files for cutting'
    $lnk.Save()
}
"#;

const REMOVE_SHORTCUTS_SCRIPT: &str = r#"
foreach ($folder in @([Environment]::GetFolderPath('Desktop'), [Environment]::GetFolderPath('Programs'))) {
    Remove-Item -LiteralPath (Join-Path $folder ($env:DXF_NAME + '.lnk')) -ErrorAction SilentlyContinue
}
"#;

fn write_app(path: &Path) -> Result<(), String> {
    std::fs::write(path, APP_EXE).map_err(|e| {
        if e.kind() == std::io::ErrorKind::PermissionDenied || e.raw_os_error() == Some(32) {
            format!("{APP_NAME} is currently open.\n\nPlease close it and run Setup again.")
        } else {
            format!("Could not write {}: {e}", path.display())
        }
    })
}

fn install(quiet: bool) -> Result<(), String> {
    let dir = install_dir()?;
    let exe = dir.join(EXE_NAME);
    let title = format!("{APP_NAME} {VERSION} Setup");
    let upgrade = exe.exists();
    if !quiet {
        let text = format!(
            "{} {APP_NAME} {VERSION}?\n\nIt is installed for your Windows account only (no admin rights needed) in:\n{}\n\nShortcuts are added to the Desktop and Start menu.",
            if upgrade { "Update" } else { "Install" },
            dir.display()
        );
        if !ask(&title, &text) {
            return Ok(());
        }
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

    let exe_s = exe.to_string_lossy().to_string();
    let dir_s = dir.to_string_lossy().to_string();
    powershell(SHORTCUT_SCRIPT, &[("DXF_EXE", &exe_s), ("DXF_DIR", &dir_s), ("DXF_NAME", APP_NAME)])?;

    let un = uninstaller.to_string_lossy();
    reg_sz("DisplayName", APP_NAME)?;
    reg_sz("DisplayVersion", VERSION)?;
    reg_sz("DisplayIcon", &format!("{exe_s},0"))?;
    reg_sz("InstallLocation", &dir_s)?;
    reg_sz("UninstallString", &format!("\"{un}\" /uninstall"))?;
    reg_sz("QuietUninstallString", &format!("\"{un}\" /uninstall /quiet"))?;
    reg_dword("NoModify", 1)?;
    reg_dword("NoRepair", 1)?;
    reg_dword("EstimatedSize", ((APP_EXE.len() * 2) / 1024) as u32)?;

    if !quiet && ask(&title, &format!("{APP_NAME} is installed.\n\nOpen it now?")) {
        let _ = Command::new(&exe).current_dir(&dir).spawn();
    }
    Ok(())
}

fn uninstall(quiet: bool) -> Result<(), String> {
    let dir = install_dir()?;
    let title = format!("{APP_NAME} Setup");
    if !quiet && !ask(&title, &format!("Remove {APP_NAME} from this computer?")) {
        return Ok(());
    }
    let exe = dir.join(EXE_NAME);
    if exe.exists() {
        std::fs::remove_file(&exe).map_err(|_| format!("{APP_NAME} is currently open.\n\nPlease close it and try again."))?;
    }
    powershell(REMOVE_SHORTCUTS_SCRIPT, &[("DXF_NAME", APP_NAME)])?;
    let _ = reg(&["delete", REG_KEY, "/f"]);

    // This program may be running from inside the install folder, so let a detached
    // cmd.exe remove the folder a moment after we exit.
    if dir.exists() {
        let temp = std::env::temp_dir();
        let _ = Command::new("cmd.exe")
            .raw_arg(format!("/c ping -n 3 127.0.0.1 >nul & rmdir /s /q \"{}\"", dir.display()))
            .current_dir(&temp)
            .creation_flags(CREATE_NO_WINDOW | DETACHED_PROCESS)
            .spawn();
    }
    if !quiet {
        message(MessageLevel::Info, &title, &format!("{APP_NAME} has been removed."));
    }
    Ok(())
}
