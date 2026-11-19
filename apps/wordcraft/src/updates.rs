//! Open the fork's update manager off the UI thread, falling back to its download page.
use wordcraft_ui_egui::app_updates::UpdateHook;

pub(super) fn hook(ctx: &egui::Context) -> Option<UpdateHook> {
    #[cfg(target_os = "macos")]
    {
        let ctx = ctx.clone();
        Some(Box::new(move || {
            let (tx, rx) = std::sync::mpsc::channel();
            let ctx = ctx.clone();
            std::thread::spawn(move || {
                let _ = tx.send(open_manager());
                ctx.request_repaint();
            });
            rx
        }))
    }
    #[cfg(not(target_os = "macos"))]
    {
        let _ = ctx;
        None
    }
}
#[cfg(target_os = "macos")]
fn open_manager() -> Result<(), String> {
    use wordcraft_ui_egui::app_updates::{DOWNLOAD_URL, MANAGER_ID};
    if launch(&["-b", MANAGER_ID]).is_ok() {
        return Ok(());
    }
    launch(&[DOWNLOAD_URL])
}

#[cfg(target_os = "macos")]
fn launch(args: &[&str]) -> Result<(), String> {
    use std::{
        process::{Command, Stdio},
        time::{Duration, Instant},
    };
    const LIMIT: Duration = Duration::from_secs(5);
    const POLL: Duration = Duration::from_millis(25);
    let failure = || "Could not open app updates. Visit https://f1nb0.dev/apps/.".to_string();
    let mut child = Command::new("/usr/bin/open").args(args).stdout(Stdio::null()).stderr(Stdio::null()).spawn().map_err(|_| failure())?;
    let started = Instant::now();
    loop {
        match child.try_wait() {
            Ok(Some(status)) => return if status.success() { Ok(()) } else { Err(failure()) },
            Ok(None) if started.elapsed() < LIMIT => std::thread::sleep(POLL),
            _ => {
                let _ = child.kill();
                let _ = child.wait();
                return Err(failure());
            }
        }
    }
}
