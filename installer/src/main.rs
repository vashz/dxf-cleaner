//! Single-file, per-user installer for DXF Converter (no admin rights needed).
//!
//!   dxf-converter-setup.exe               interactive install / update
//!   dxf-converter-setup.exe /quiet        silent install (for IT / scripts)
//!   uninstall.exe /uninstall [/quiet]     remove (used by Windows "Apps & features")
//!
//! Installs to %LOCALAPPDATA%\Programs\DXF Converter, adds Desktop and Start-menu
//! shortcuts and registers an uninstall entry under HKCU.
//!
//! `flow` decides what to do and in which order; `windows` is the `Host` that does it.

#![windows_subsystem = "windows"]

mod flow;
mod version;
mod windows;

const VERSION: &str = env!("DXF_APP_VERSION", "set DXF_APP_VERSION (use package.ps1)");

fn main() {
    let args: Vec<String> = std::env::args().skip(1).map(|a| a.to_ascii_lowercase()).collect();
    let quiet = args.iter().any(|a| matches!(a.as_str(), "/quiet" | "/s" | "/silent" | "--quiet"));
    let remove = args.iter().any(|a| matches!(a.as_str(), "/uninstall" | "--uninstall" | "/u"));
    let result = windows::install_dir().and_then(|dir| {
        let setup = flow::Setup { version: VERSION.to_string(), dir, quiet };
        let host = &mut windows::WindowsHost;
        if remove { flow::uninstall(host, &setup) } else { flow::install(host, &setup) }
    });
    if let Err(msg) = result {
        if !quiet {
            windows::show_error(&format!("{} Setup", flow::APP_NAME), &msg);
        }
        std::process::exit(1);
    }
}
