//! What Setup decides and in which order it acts. Every effect goes through `Host`, so the
//! whole Install / Update / Reinstall / Downgrade / Uninstall matrix is testable without
//! touching this PC (see the recording host in the tests).

use crate::version::version_parts;
use std::cmp::Ordering;
use std::path::{Path, PathBuf};

pub const APP_NAME: &str = "DXF Cleaner";
pub const EXE_NAME: &str = "DXF Cleaner.exe";
pub const UNINSTALLER: &str = "uninstall.exe";
/// The app's name before 0.2.0. Setup moves an install under it over to the new name.
pub const OLD_APP_NAME: &str = "DXF Converter";
pub const OLD_EXE_NAME: &str = "DXF Converter.exe";

/// Where an install under the old name lives: next to `dir`, in a folder named after it.
pub fn old_install_dir(dir: &Path) -> PathBuf {
    dir.with_file_name(OLD_APP_NAME)
}

/// What this run of Setup is: the version it carries, where it installs, and whether it may ask.
pub struct Setup {
    pub version: String,
    pub dir: PathBuf,
    pub quiet: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Action {
    Install,
    Update,
    Reinstall,
    Downgrade,
}

impl Action {
    fn verb(self) -> &'static str {
        match self {
            Action::Install => "Install",
            Action::Update => "Update",
            Action::Reinstall => "Reinstall",
            Action::Downgrade => "Downgrade",
        }
    }
}

/// `installed` is only known when the app is present; a present app with no recorded
/// version is reinstalled.
pub fn decide(present: bool, installed: Option<&str>, setup: &str) -> Action {
    match installed {
        None if present => Action::Reinstall,
        None => Action::Install,
        Some(old) => match version_parts(old).cmp(&version_parts(setup)) {
            Ordering::Less => Action::Update,
            Ordering::Equal => Action::Reinstall,
            Ordering::Greater => Action::Downgrade,
        },
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Question {
    /// Install, Update or Reinstall.
    Confirm { action: Action, version: String, dir: PathBuf },
    ConfirmDowngrade { installed: String, version: String },
    /// Asked after installing; `shortcut_error` says why the shortcuts are missing.
    OpenNow { version: String, exe: PathBuf, shortcut_error: Option<String> },
    ConfirmRemove,
}

impl Question {
    pub fn title(&self) -> String {
        match self {
            Question::Confirm { version, .. } | Question::ConfirmDowngrade { version, .. } | Question::OpenNow { version, .. } => {
                format!("{APP_NAME} {version} Setup")
            }
            Question::ConfirmRemove => format!("{APP_NAME} Setup"),
        }
    }

    pub fn text(&self) -> String {
        match self {
            Question::Confirm { action, version, dir } => format!(
                "{} {APP_NAME} {version}?\n\nIt is installed for your Windows account only (no admin rights needed) in:\n{}\n\nShortcuts are added to the Desktop and Start menu.",
                action.verb(),
                dir.display()
            ),
            Question::ConfirmDowngrade { installed, version } => {
                format!("{}\n\nReplace it with the older version {version}?", newer_installed(installed, version))
            }
            Question::OpenNow { shortcut_error: None, .. } => format!("{APP_NAME} is installed.\n\nOpen it now?"),
            Question::OpenNow { exe, shortcut_error: Some(e), .. } => format!(
                "{APP_NAME} is installed, but the shortcuts could not be created:\n{e}\n\nYou can start it from:\n{}\n\nOpen it now?",
                exe.display()
            ),
            Question::ConfirmRemove => format!("Remove {APP_NAME} from this computer?"),
        }
    }

    pub fn is_warning(&self) -> bool {
        matches!(self, Question::ConfirmDowngrade { .. })
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Notice {
    Removed,
}

impl Notice {
    pub fn title(&self) -> String {
        format!("{APP_NAME} Setup")
    }

    pub fn text(&self) -> String {
        match self {
            Notice::Removed => format!("{APP_NAME} has been removed."),
        }
    }
}

fn newer_installed(installed: &str, version: &str) -> String {
    format!("A newer version of {APP_NAME} ({installed}) is already installed.\n\nThis setup contains version {version}.")
}

fn close_app_message() -> String {
    format!("{APP_NAME} is currently open.\n\nPlease close it and try again.")
}

/// Everything Setup needs from the PC it runs on.
pub trait Host {
    fn ask(&mut self, q: &Question) -> bool;
    fn tell(&mut self, n: &Notice);
    /// Is the app's exe in `dir`?
    fn app_present(&self, dir: &Path) -> bool;
    /// Is the app installed under its old name in `old_dir`?
    fn old_install_present(&self, old_dir: &Path) -> bool;
    /// Version recorded by a previous install, if any.
    fn installed_version(&self) -> Option<String>;
    /// Is the app open? An error means it could not be checked.
    fn app_running(&self) -> Result<bool, String>;
    fn write_app(&mut self, dir: &Path) -> Result<(), String>;
    fn copy_uninstaller(&mut self, dir: &Path) -> Result<(), String>;
    /// Add the "Apps & features" entry.
    fn register(&mut self, dir: &Path, version: &str) -> Result<(), String>;
    fn create_shortcuts(&mut self, dir: &Path) -> Result<(), String>;
    fn launch(&mut self, dir: &Path);
    fn remove_app(&mut self, dir: &Path) -> Result<(), String>;
    fn remove_shortcuts(&mut self) -> Result<(), String>;
    fn unregister(&mut self);
    fn schedule_folder_removal(&mut self, dir: &Path);
    /// Remove an install under the old name: its exe, its shortcuts and, once Setup has
    /// exited, its folder. The "Apps & features" entry is shared, so it stays.
    fn remove_old_install(&mut self, old_dir: &Path) -> Result<(), String>;
}

pub fn install(host: &mut impl Host, setup: &Setup) -> Result<(), String> {
    let old_dir = old_install_dir(&setup.dir);
    let old_install = host.old_install_present(&old_dir);
    // An install under the old name counts as the app being present, so it is updated.
    let present = host.app_present(&setup.dir) || old_install;
    let installed = if present { host.installed_version() } else { None };
    let action = decide(present, installed.as_deref(), &setup.version);

    let question = match action {
        Action::Downgrade => {
            // Never silently replace a newer version with an older one.
            let installed = installed.unwrap_or_default();
            if setup.quiet {
                return Err(newer_installed(&installed, &setup.version));
            }
            Some(Question::ConfirmDowngrade { installed, version: setup.version.clone() })
        }
        _ if setup.quiet => None,
        _ => Some(Question::Confirm { action, version: setup.version.clone(), dir: setup.dir.clone() }),
    };
    if let Some(q) = question {
        if !host.ask(&q) {
            return Ok(());
        }
    }
    ensure_closed(host)?;

    host.write_app(&setup.dir)?;
    host.copy_uninstaller(&setup.dir)?;
    // Register first, so the install can always be removed from "Apps & features"
    // even if the shortcut step fails (e.g. on a locked-down PC).
    host.register(&setup.dir, &setup.version)?;
    let shortcuts = host.create_shortcuts(&setup.dir);
    // Only once the new app is in place, so a failure here never leaves no app at all.
    let old_install_removed = if old_install { host.remove_old_install(&old_dir) } else { Ok(()) };

    if !setup.quiet {
        let q = Question::OpenNow {
            version: setup.version.clone(),
            exe: setup.dir.join(EXE_NAME),
            shortcut_error: shortcuts.as_ref().err().cloned(),
        };
        if host.ask(&q) {
            host.launch(&setup.dir);
        }
    }
    both(shortcuts, old_install_removed)
}

/// Refuse while the app is open. If that cannot be checked, carry on: a locked exe still
/// makes `write_app` / `remove_app` fail with a clear message.
fn ensure_closed(host: &impl Host) -> Result<(), String> {
    if host.app_running().unwrap_or(false) { Err(close_app_message()) } else { Ok(()) }
}

pub fn uninstall(host: &mut impl Host, setup: &Setup) -> Result<(), String> {
    if !setup.quiet && !host.ask(&Question::ConfirmRemove) {
        return Ok(());
    }
    ensure_closed(host)?;
    host.remove_app(&setup.dir)?;
    host.remove_shortcuts()?;
    // No `?`: the app is already gone, so its "Apps & features" entry must go as well,
    // even when the old install cannot be removed. That failure is reported at the end.
    let old_dir = old_install_dir(&setup.dir);
    let old_install_removed = if host.old_install_present(&old_dir) { host.remove_old_install(&old_dir) } else { Ok(()) };
    host.unregister();
    // Tell the user first: this program may live inside the install folder and keeps
    // its own file locked until it exits.
    if !setup.quiet {
        host.tell(&Notice::Removed);
    }
    host.schedule_folder_removal(&setup.dir);
    old_install_removed
}

/// Both results of steps that run whatever the other's outcome; neither error is lost.
fn both(a: Result<(), String>, b: Result<(), String>) -> Result<(), String> {
    match (a, b) {
        (Ok(()), r) | (r, Ok(())) => r,
        (Err(a), Err(b)) => Err(format!("{a}\n\n{b}")),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::VecDeque;

    #[derive(Debug, Clone, PartialEq, Eq)]
    enum Effect {
        Asked(Question),
        Told(Notice),
        WriteApp,
        CopyUninstaller,
        Register(String),
        CreateShortcuts,
        Launch,
        RemoveApp,
        RemoveShortcuts,
        Unregister,
        ScheduleFolderRemoval,
        RemoveOldInstall(PathBuf),
    }
    use Effect::*;

    /// A PC that answers from a script and records every effect in order.
    struct Recording {
        present: bool,
        old_install: bool,
        old_install_removal: Result<(), String>,
        installed: Option<String>,
        running: Result<bool, String>,
        shortcuts: Result<(), String>,
        answers: VecDeque<bool>,
        log: Vec<Effect>,
    }

    impl Recording {
        fn new() -> Self {
            Recording { present: false, old_install: false, old_install_removal: Ok(()), installed: None, running: Ok(false), shortcuts: Ok(()), answers: VecDeque::new(), log: Vec::new() }
        }
        fn with_app(mut self, version: Option<&str>) -> Self {
            self.present = true;
            self.installed = version.map(str::to_string);
            self
        }
        /// The app is installed under its old name only, at `version`.
        fn with_old_install(mut self, version: &str) -> Self {
            self.old_install = true;
            self.installed = Some(version.to_string());
            self
        }
        fn answering(mut self, answers: &[bool]) -> Self {
            self.answers = answers.iter().copied().collect();
            self
        }
    }

    impl Host for Recording {
        fn ask(&mut self, q: &Question) -> bool {
            self.log.push(Asked(q.clone()));
            self.answers.pop_front().unwrap_or_else(|| panic!("unexpected question: {q:?}"))
        }
        fn tell(&mut self, n: &Notice) {
            self.log.push(Told(n.clone()));
        }
        fn app_present(&self, _: &Path) -> bool {
            self.present
        }
        fn old_install_present(&self, _: &Path) -> bool {
            self.old_install
        }
        fn installed_version(&self) -> Option<String> {
            self.installed.clone()
        }
        fn app_running(&self) -> Result<bool, String> {
            self.running.clone()
        }
        fn write_app(&mut self, _: &Path) -> Result<(), String> {
            self.log.push(WriteApp);
            Ok(())
        }
        fn copy_uninstaller(&mut self, _: &Path) -> Result<(), String> {
            self.log.push(CopyUninstaller);
            Ok(())
        }
        fn register(&mut self, _: &Path, version: &str) -> Result<(), String> {
            self.log.push(Register(version.to_string()));
            Ok(())
        }
        fn create_shortcuts(&mut self, _: &Path) -> Result<(), String> {
            self.log.push(CreateShortcuts);
            self.shortcuts.clone()
        }
        fn launch(&mut self, _: &Path) {
            self.log.push(Launch);
        }
        fn remove_app(&mut self, _: &Path) -> Result<(), String> {
            self.log.push(RemoveApp);
            Ok(())
        }
        fn remove_shortcuts(&mut self) -> Result<(), String> {
            self.log.push(RemoveShortcuts);
            Ok(())
        }
        fn unregister(&mut self) {
            self.log.push(Unregister);
        }
        fn schedule_folder_removal(&mut self, _: &Path) {
            self.log.push(ScheduleFolderRemoval);
        }
        fn remove_old_install(&mut self, old_dir: &Path) -> Result<(), String> {
            self.log.push(RemoveOldInstall(old_dir.to_path_buf()));
            self.old_install_removal.clone()
        }
    }

    fn setup(version: &str, quiet: bool) -> Setup {
        Setup { version: version.to_string(), dir: PathBuf::from(r"C:\Users\staff\AppData\Local\Programs\DXF Cleaner"), quiet }
    }

    fn confirm(action: Action, s: &Setup) -> Effect {
        Asked(Question::Confirm { action, version: s.version.clone(), dir: s.dir.clone() })
    }

    fn open_now(s: &Setup, shortcut_error: Option<&str>) -> Effect {
        Asked(Question::OpenNow { version: s.version.clone(), exe: s.dir.join(EXE_NAME), shortcut_error: shortcut_error.map(str::to_string) })
    }

    #[test]
    fn fresh_install_writes_registers_then_adds_shortcuts_and_offers_to_open() {
        let s = setup("0.1.2", false);
        let mut pc = Recording::new().answering(&[true, true]);
        assert_eq!(install(&mut pc, &s), Ok(()));
        assert_eq!(
            pc.log,
            [confirm(Action::Install, &s), WriteApp, CopyUninstaller, Register("0.1.2".into()), CreateShortcuts, open_now(&s, None), Launch]
        );
    }

    #[test]
    fn uninstall_removes_app_shortcuts_entry_then_tells_before_removing_the_folder() {
        let mut pc = Recording::new().with_app(Some("0.1.2")).answering(&[true]);
        assert_eq!(uninstall(&mut pc, &setup("0.1.2", false)), Ok(()));
        assert_eq!(
            pc.log,
            [Asked(Question::ConfirmRemove), RemoveApp, RemoveShortcuts, Unregister, Told(Notice::Removed), ScheduleFolderRemoval]
        );
    }

    /// The first question Setup asks for this PC, answered No.
    fn first_question(pc: Recording, version: &str) -> Effect {
        let mut pc = pc.answering(&[false]);
        assert_eq!(install(&mut pc, &setup(version, false)), Ok(()));
        assert_eq!(pc.log.len(), 1, "a No must stop before any effect: {:?}", pc.log);
        pc.log.remove(0)
    }

    #[test]
    fn installed_version_picks_update_or_reinstall() {
        let s = setup("0.1.2", false);
        assert_eq!(first_question(Recording::new().with_app(Some("0.1.1")), "0.1.2"), confirm(Action::Update, &s));
        assert_eq!(first_question(Recording::new().with_app(Some("0.1.2")), "0.1.2"), confirm(Action::Reinstall, &s));
        assert_eq!(first_question(Recording::new().with_app(None), "0.1.2"), confirm(Action::Reinstall, &s));
        let short = setup("0.1", false);
        assert_eq!(first_question(Recording::new().with_app(Some("0.1.0")), "0.1"), confirm(Action::Reinstall, &short));
    }

    #[test]
    fn a_recorded_version_without_the_app_is_a_fresh_install() {
        let mut pc = Recording::new();
        pc.installed = Some("0.2.0".into());
        assert_eq!(first_question(pc, "0.1.2"), confirm(Action::Install, &setup("0.1.2", false)));
    }

    #[test]
    fn downgrade_asks_with_a_warning_and_yes_installs_the_older_version() {
        let s = setup("0.1.1", false);
        let mut pc = Recording::new().with_app(Some("0.1.2")).answering(&[true, false]);
        assert_eq!(install(&mut pc, &s), Ok(()));
        let ask = Question::ConfirmDowngrade { installed: "0.1.2".into(), version: "0.1.1".into() };
        assert!(ask.is_warning());
        assert_eq!(pc.log, [Asked(ask), WriteApp, CopyUninstaller, Register("0.1.1".into()), CreateShortcuts, open_now(&s, None)]);
    }

    #[test]
    fn downgrade_answered_no_changes_nothing() {
        let asked = first_question(Recording::new().with_app(Some("0.1.2")), "0.1.1");
        assert_eq!(asked, Asked(Question::ConfirmDowngrade { installed: "0.1.2".into(), version: "0.1.1".into() }));
    }

    #[test]
    fn quiet_downgrade_is_refused_without_touching_anything() {
        let mut pc = Recording::new().with_app(Some("0.1.2"));
        let err = install(&mut pc, &setup("0.1.1", true)).unwrap_err();
        assert!(err.contains("(0.1.2) is already installed") && err.contains("version 0.1.1"), "{err}");
        assert_eq!(pc.log, []);
    }

    #[test]
    fn quiet_install_asks_nothing() {
        let mut pc = Recording::new().with_app(Some("0.1.1"));
        assert_eq!(install(&mut pc, &setup("0.1.2", true)), Ok(()));
        assert_eq!(pc.log, [WriteApp, CopyUninstaller, Register("0.1.2".into()), CreateShortcuts]);
    }

    #[test]
    fn open_app_blocks_install_after_the_confirmation_and_writes_nothing() {
        let s = setup("0.1.2", false);
        let mut pc = Recording::new().answering(&[true]);
        pc.running = Ok(true);
        assert_eq!(install(&mut pc, &s), Err(close_app_message()));
        assert_eq!(pc.log, [confirm(Action::Install, &s)]);
    }

    #[test]
    fn install_carries_on_when_the_open_app_check_itself_fails() {
        let mut pc = Recording::new();
        pc.running = Err("tasklist failed".into());
        assert_eq!(install(&mut pc, &setup("0.1.2", true)), Ok(()));
        assert!(pc.log.contains(&WriteApp));
    }

    #[test]
    fn failed_shortcuts_keep_the_registration_and_say_where_the_app_is() {
        let s = setup("0.1.2", false);
        let mut pc = Recording::new().answering(&[true, false]);
        pc.shortcuts = Err("PowerShell failed: blocked".into());
        assert_eq!(install(&mut pc, &s), Err("PowerShell failed: blocked".into()));
        assert_eq!(
            pc.log,
            [confirm(Action::Install, &s), WriteApp, CopyUninstaller, Register("0.1.2".into()), CreateShortcuts, open_now(&s, Some("PowerShell failed: blocked"))]
        );
    }

    #[test]
    fn quiet_install_with_failed_shortcuts_reports_the_error() {
        let mut pc = Recording::new();
        pc.shortcuts = Err("PowerShell failed: blocked".into());
        assert_eq!(install(&mut pc, &setup("0.1.2", true)), Err("PowerShell failed: blocked".into()));
        assert!(pc.log.contains(&Register("0.1.2".into())));
    }

    #[test]
    fn uninstall_answered_no_changes_nothing() {
        let mut pc = Recording::new().with_app(Some("0.1.2")).answering(&[false]);
        assert_eq!(uninstall(&mut pc, &setup("0.1.2", false)), Ok(()));
        assert_eq!(pc.log, [Asked(Question::ConfirmRemove)]);
    }

    #[test]
    fn quiet_uninstall_neither_asks_nor_tells() {
        let mut pc = Recording::new().with_app(Some("0.1.2"));
        assert_eq!(uninstall(&mut pc, &setup("0.1.2", true)), Ok(()));
        assert_eq!(pc.log, [RemoveApp, RemoveShortcuts, Unregister, ScheduleFolderRemoval]);
    }

    #[test]
    fn open_app_blocks_uninstall() {
        let mut pc = Recording::new().with_app(Some("0.1.2"));
        pc.running = Ok(true);
        assert_eq!(uninstall(&mut pc, &setup("0.1.2", true)), Err(close_app_message()));
        assert_eq!(pc.log, []);
    }

    #[test]
    fn questions_read_as_staff_see_them() {
        let q = Question::Confirm { action: Action::Update, version: "0.1.2".into(), dir: PathBuf::from(r"C:\Apps\DXF Cleaner") };
        assert_eq!(q.title(), "DXF Cleaner 0.1.2 Setup");
        assert_eq!(
            q.text(),
            "Update DXF Cleaner 0.1.2?\n\nIt is installed for your Windows account only (no admin rights needed) in:\nC:\\Apps\\DXF Cleaner\n\nShortcuts are added to the Desktop and Start menu."
        );
        let q = Question::OpenNow { version: "0.1.2".into(), exe: PathBuf::from(r"C:\Apps\DXF Cleaner.exe"), shortcut_error: Some("blocked".into()) };
        assert_eq!(
            q.text(),
            "DXF Cleaner is installed, but the shortcuts could not be created:\nblocked\n\nYou can start it from:\nC:\\Apps\\DXF Cleaner.exe\n\nOpen it now?"
        );
        assert_eq!(Question::ConfirmRemove.title(), "DXF Cleaner Setup");
    }

    fn old_dir() -> PathBuf {
        PathBuf::from(r"C:\Users\staff\AppData\Local\Programs\DXF Converter")
    }

    #[test]
    fn the_old_install_sits_next_to_the_new_one() {
        assert_eq!(old_install_dir(&setup("0.2.0", false).dir), old_dir());
    }

    #[test]
    fn an_install_under_the_old_name_is_updated_and_then_removed() {
        let s = setup("0.2.0", false);
        let mut pc = Recording::new().with_old_install("0.1.4").answering(&[true, false]);
        assert_eq!(install(&mut pc, &s), Ok(()));
        assert_eq!(
            pc.log,
            [
                confirm(Action::Update, &s),
                WriteApp,
                CopyUninstaller,
                Register("0.2.0".into()),
                CreateShortcuts,
                RemoveOldInstall(old_dir()),
                open_now(&s, None)
            ]
        );
    }

    #[test]
    fn a_failed_old_install_removal_is_reported_but_keeps_the_new_app() {
        let mut pc = Recording::new().with_old_install("0.1.4");
        pc.old_install_removal = Err("Could not remove DXF Converter.exe".into());
        assert_eq!(install(&mut pc, &setup("0.2.0", true)), Err("Could not remove DXF Converter.exe".into()));
        assert_eq!(pc.log, [WriteApp, CopyUninstaller, Register("0.2.0".into()), CreateShortcuts, RemoveOldInstall(old_dir())]);
    }

    #[test]
    fn the_old_app_open_blocks_the_update() {
        let mut pc = Recording::new().with_old_install("0.1.4");
        pc.running = Ok(true);
        assert_eq!(install(&mut pc, &setup("0.2.0", true)), Err(close_app_message()));
        assert_eq!(pc.log, []);
    }

    #[test]
    fn uninstall_also_removes_an_old_install_left_behind() {
        let mut pc = Recording::new().with_app(Some("0.2.0"));
        pc.old_install = true;
        assert_eq!(uninstall(&mut pc, &setup("0.2.0", true)), Ok(()));
        assert_eq!(pc.log, [RemoveApp, RemoveShortcuts, RemoveOldInstall(old_dir()), Unregister, ScheduleFolderRemoval]);
    }

    #[test]
    fn uninstall_still_removes_the_entry_when_the_old_install_cannot_be_removed() {
        let mut pc = Recording::new().with_app(Some("0.2.0"));
        pc.old_install = true;
        pc.old_install_removal = Err("Could not remove DXF Converter.exe".into());
        assert_eq!(uninstall(&mut pc, &setup("0.2.0", true)), Err("Could not remove DXF Converter.exe".into()));
        assert_eq!(pc.log, [RemoveApp, RemoveShortcuts, RemoveOldInstall(old_dir()), Unregister, ScheduleFolderRemoval]);
    }

    #[test]
    fn failed_shortcuts_and_old_install_removal_report_both() {
        let mut pc = Recording::new().with_old_install("0.1.4");
        pc.shortcuts = Err("PowerShell failed: blocked".into());
        pc.old_install_removal = Err("Could not remove DXF Converter.exe".into());
        assert_eq!(
            install(&mut pc, &setup("0.2.0", true)),
            Err("PowerShell failed: blocked\n\nCould not remove DXF Converter.exe".into())
        );
    }
}
