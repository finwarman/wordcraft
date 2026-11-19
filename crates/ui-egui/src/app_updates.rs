//! Update-manager action. The host owns process I/O; the UI retains one pending answer.
use crate::WordApp;
use serde_json::{Value, json};
use std::sync::mpsc::{Receiver, TryRecvError};

pub const COMMAND: &str = "ui.appUpdates";
pub const MANAGER_ID: &str = "dev.f1nb0.launcher";
pub const DOWNLOAD_URL: &str = "https://f1nb0.dev/apps/";
pub type UpdateHook = Box<dyn Fn() -> Receiver<Result<(), String>>>;

impl WordApp {
    pub(crate) fn updates_ready(&self) -> bool {
        self.update_request.is_none()
    }
    pub(crate) fn open_app_updates(&mut self) -> Value {
        if self.update_request.is_some() {
            return json!({"pending":true});
        }
        if let Some(open) = &self.services.app_updates {
            self.update_request = Some(open());
            self.status("Opening f1nb0 Apps…");
            return json!({"pending":true});
        }
        self.canvas.open_url = Some(DOWNLOAD_URL.into());
        json!({"pending":false,"url":DOWNLOAD_URL})
    }
    pub(crate) fn poll_app_updates(&mut self) {
        let Some(reply) = &self.update_request else { return };
        let result = match reply.try_recv() {
            Ok(result) => result,
            Err(TryRecvError::Empty) => return,
            Err(TryRecvError::Disconnected) => Err("Update manager did not answer. Try again.".into()),
        };
        self.update_request = None;
        self.status(match result {
            Ok(()) => "App updates opened.".into(),
            Err(error) => error,
        });
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    use crate::Services;
    #[test]
    fn failed_launch_recovers() {
        let (tx, rx) = std::sync::mpsc::channel();
        let rx = std::cell::RefCell::new(Some(rx));
        let services = Services { app_updates: Some(Box::new(move || rx.borrow_mut().take().unwrap())), ..Default::default() };
        let mut app = WordApp::new(wordcraft_engine::Session::new(wordcraft_doc::Document::new()), services);
        assert_eq!(app.run(COMMAND, json!({})).unwrap()["pending"], true);
        assert!(!app.updates_ready());
        assert_eq!(app.run(COMMAND, json!({})).unwrap()["pending"], true);
        tx.send(Err("Synthetic launch failure".into())).unwrap();
        app.poll_app_updates();
        assert!(app.updates_ready());
        assert_eq!(app.status_msg.as_ref().map(|s| s.0.as_str()), Some("Synthetic launch failure"));
    }
    #[test]
    fn fallback_opens_downloads() {
        let mut app = WordApp::new(wordcraft_engine::Session::new(wordcraft_doc::Document::new()), Services::default());
        app.run(COMMAND, json!({})).unwrap();
        assert_eq!(app.canvas.open_url.as_deref(), Some(DOWNLOAD_URL));
    }
}
