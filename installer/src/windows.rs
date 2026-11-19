//! The real `Host`: dialogs, files, registry and shortcuts on this Windows account.

use crate::flow::{APP_NAME, EXE_NAME, Host, Notice, OLD_APP_NAME, OLD_EXE_NAME, Question, UNINSTALLER};
use rfd::{MessageButtons, MessageDialog, MessageDialogResult, MessageLevel};
use std::os::windows::process::CommandExt;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::atomic::{AtomicUsize, Ordering};

const APP_EXE: &[u8] = include_bytes!(env!("DXF_APP_EXE", "set DXF_APP_EXE (use package.ps1)"));
const PUBLISHER: &str = "NHL";
/// Kept from before the rename to DXF Cleaner: it is the product's identity in "Apps &
/// features" (staff see DisplayName, not this), so an old install's version is still found.
const REG_KEY: &str = r"HKCU\Software\Microsoft\Windows\CurrentVersion\Uninstall\NHL-DXF-Converter";
const CREATE_NO_WINDOW: u32 = 0x0800_0000;
const DETACHED_PROCESS: u32 = 0x0000_0008;

pub struct WindowsHost;

impl Host for WindowsHost {
    fn ask(&mut self, q: &Question) -> bool {
        let level = if q.is_warning() { MessageLevel::Warning } else { MessageLevel::Info };
        matches!(
            MessageDialog::new()
                .set_level(level)
                .set_title(q.title())
                .set_description(q.text())
                .set_buttons(MessageButtons::YesNo)
                .show(),
            MessageDialogResult::Yes
        )
    }

    fn tell(&mut self, n: &Notice) {
        message(MessageLevel::Info, &n.title(), &n.text());
    }

    fn app_present(&self, dir: &Path) -> bool {
        dir.join(EXE_NAME).exists()
    }

    fn old_install_present(&self, old_dir: &Path) -> bool {
        old_dir.join(OLD_EXE_NAME).exists()
    }

    fn installed_version(&self) -> Option<String> {
        let out = reg(&["query", REG_KEY, "/v", "DisplayVersion"]).ok()?;
        out.lines()
            .find(|l| l.contains("DisplayVersion"))
            .and_then(|l| l.split_whitespace().last())
            .map(str::to_string)
    }

    /// Under either name: the old app holds files that an update from it removes.
    fn app_running(&self) -> Result<bool, String> {
        for exe in [EXE_NAME, OLD_EXE_NAME] {
            let filter = format!("IMAGENAME eq {exe}");
            let out = run_tool("tasklist", Command::new(system_tool("tasklist.exe")).args(["/FI", &filter, "/NH", "/FO", "CSV"]))?;
            if out.to_ascii_lowercase().contains(&exe.to_ascii_lowercase()) {
                return Ok(true);
            }
        }
        Ok(false)
    }

    /// Write the app next to its final name, then swap it in, so a failed write never
    /// leaves a half-written exe behind.
    fn write_app(&mut self, dir: &Path) -> Result<(), String> {
        std::fs::create_dir_all(dir).map_err(|e| format!("Could not create {}: {e}", dir.display()))?;
        let path = dir.join(EXE_NAME);
        let tmp = path.with_extension("exe.new");
        std::fs::write(&tmp, APP_EXE).map_err(|e| format!("Could not write {}: {e}", tmp.display()))?;
        std::fs::rename(&tmp, &path).map_err(|e| {
            let _ = std::fs::remove_file(&tmp);
            format!("Could not replace {}: {e}", path.display())
        })
    }

    /// Keep a copy of this setup program as the uninstaller.
    fn copy_uninstaller(&mut self, dir: &Path) -> Result<(), String> {
        let me = std::env::current_exe().map_err(|e| e.to_string())?;
        let uninstaller = dir.join(UNINSTALLER);
        let same = std::fs::canonicalize(&me).ok() == std::fs::canonicalize(&uninstaller).ok();
        if !same {
            std::fs::copy(&me, &uninstaller).map_err(|e| format!("Could not write the uninstaller: {e}"))?;
        }
        Ok(())
    }

    fn register(&mut self, dir: &Path, version: &str) -> Result<(), String> {
        let exe = dir.join(EXE_NAME).to_string_lossy().to_string();
        let un = dir.join(UNINSTALLER).to_string_lossy().to_string();
        reg_sz("DisplayName", APP_NAME)?;
        reg_sz("DisplayVersion", version)?;
        reg_sz("Publisher", PUBLISHER)?;
        reg_sz("DisplayIcon", &format!("{exe},0"))?;
        reg_sz("InstallLocation", &dir.to_string_lossy())?;
        reg_sz("UninstallString", &format!("\"{un}\" /uninstall"))?;
        reg_sz("QuietUninstallString", &format!("\"{un}\" /uninstall /quiet"))?;
        reg_dword("NoModify", 1)?;
        reg_dword("NoRepair", 1)?;
        reg_dword("EstimatedSize", ((APP_EXE.len() * 2) / 1024) as u32)
    }

    fn create_shortcuts(&mut self, dir: &Path) -> Result<(), String> {
        let exe = dir.join(EXE_NAME).to_string_lossy().to_string();
        let dir = dir.to_string_lossy().to_string();
        powershell(&shortcut_script(), &[("DXF_EXE", &exe), ("DXF_DIR", &dir), ("DXF_NAME", APP_NAME)])
    }

    fn launch(&mut self, dir: &Path) {
        let _ = Command::new(dir.join(EXE_NAME)).current_dir(dir).spawn();
    }

    fn remove_app(&mut self, dir: &Path) -> Result<(), String> {
        let exe = dir.join(EXE_NAME);
        if exe.exists() {
            std::fs::remove_file(&exe).map_err(|e| format!("Could not remove {}: {e}", exe.display()))?;
        }
        Ok(())
    }

    fn remove_shortcuts(&mut self) -> Result<(), String> {
        powershell(&remove_shortcuts_script(), &[("DXF_NAME", APP_NAME)])
    }

    fn unregister(&mut self) {
        let _ = reg(&["delete", REG_KEY, "/f"]);
    }

    /// This program may live inside the install folder and keeps its own file locked until it
    /// exits, so a detached batch script removes the folder after we are gone.
    fn schedule_folder_removal(&mut self, dir: &Path) {
        if !dir.exists() {
            return;
        }
        // One script per folder: an uninstall can schedule the old folder and the new one.
        static SCRIPTS: AtomicUsize = AtomicUsize::new(0);
        let n = SCRIPTS.fetch_add(1, Ordering::Relaxed);
        let script = std::env::temp_dir().join(format!("dxf-cleaner-uninstall-{}-{n}.cmd", std::process::id()));
        if std::fs::write(&script, cleanup_script(dir, &system_tool("PING.EXE"))).is_err() {
            return;
        }
        let _ = Command::new(system_tool("cmd.exe"))
            .raw_arg(format!("/d /c \"{}\"", script.display()))
            .current_dir(std::env::temp_dir())
            .creation_flags(CREATE_NO_WINDOW | DETACHED_PROCESS)
            .spawn();
    }

    fn remove_old_install(&mut self, old_dir: &Path) -> Result<(), String> {
        let exe = old_dir.join(OLD_EXE_NAME);
        if exe.exists() {
            std::fs::remove_file(&exe).map_err(|e| format!("Could not remove {}: {e}", exe.display()))?;
        }
        powershell(&remove_shortcuts_script(), &[("DXF_NAME", OLD_APP_NAME)])?;
        self.schedule_folder_removal(old_dir);
        Ok(())
    }
}

pub fn install_dir() -> Result<PathBuf, String> {
    let base = std::env::var_os("LOCALAPPDATA").ok_or("LOCALAPPDATA is not set")?;
    let base = PathBuf::from(base);
    if !base.is_absolute() {
        return Err("LOCALAPPDATA is not an absolute path".into());
    }
    Ok(base.join("Programs").join(APP_NAME))
}

pub fn show_error(title: &str, text: &str) {
    message(MessageLevel::Error, title, text);
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn cleanup_script_removes_folder_and_itself() {
        let base = std::env::temp_dir().join(format!("dxf cleanup test {}", std::process::id()));
        let dir = base.join("DXF Cleaner");
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
}
