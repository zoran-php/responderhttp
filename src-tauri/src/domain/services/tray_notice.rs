// http_client/src-tauri/src/domain/services/tray_notice.rs
//
// Whether to tell the user that closing the window did not quit the app.
//
// The app hides to the tray on close (desktop/window.rs). Without a word,
// that looks like the app ignored the X — so the first closes show a toast,
// and its "Don't show this again" button turns it off for good.
//
// The rule lives here rather than in desktop/ because it is a stored
// preference, and desktop/ is shell only (CLAUDE.md section 3). desktop/ asks
// this service what to do; this service never touches a window.
use std::sync::Arc;

use crate::domain::ports::AppSettingsRepository;

/// The settings key. Named for what it records — the user dismissed the
/// notice — rather than for what the code does with it.
pub const TRAY_NOTICE_DISMISSED: &str = "tray_close_notice_dismissed";

const TRUE: &str = "true";

pub struct TrayNotice {
    settings: Arc<dyn AppSettingsRepository>,
}

impl TrayNotice {
    pub fn new(settings: Arc<dyn AppSettingsRepository>) -> Self {
        Self { settings }
    }

    /// Whether the close-to-tray notice should be shown.
    ///
    /// A storage failure answers "yes". The two ways to be wrong are not
    /// equal: showing a notice the user has dismissed is a small annoyance,
    /// while silently swallowing it leaves someone believing they quit an app
    /// that is still running. Fail towards telling them.
    pub fn should_show(&self) -> bool {
        match self.settings.get(TRAY_NOTICE_DISMISSED) {
            Ok(Some(value)) => value != TRUE,
            Ok(None) => true,
            Err(error) => {
                log::warn!("tray notice: could not read the preference: {error}");
                true
            }
        }
    }

    /// Records that the user asked not to see the notice again.
    ///
    /// Returns nothing: this is called from a toast callback on a Windows
    /// event thread, which has nowhere to report an error to. A failed write
    /// means the notice appears again next time, which is the harmless
    /// direction.
    pub fn never_show_again(&self) {
        if let Err(error) = self.settings.set(TRAY_NOTICE_DISMISSED, TRUE) {
            log::warn!("tray notice: could not save the preference: {error}");
        }
    }
}

#[cfg(test)]
mod tests {
    use std::sync::Mutex;

    use super::*;
    use crate::domain::error::AppError;

    #[derive(Default)]
    struct FakeSettings {
        value: Mutex<Option<String>>,
        fail: bool,
    }

    impl AppSettingsRepository for FakeSettings {
        fn get(&self, key: &str) -> Result<Option<String>, AppError> {
            assert_eq!(key, TRAY_NOTICE_DISMISSED);
            if self.fail {
                return Err(AppError::Storage("no database".into()));
            }
            Ok(self.value.lock().expect("lock").clone())
        }

        fn set(&self, key: &str, value: &str) -> Result<(), AppError> {
            assert_eq!(key, TRAY_NOTICE_DISMISSED);
            if self.fail {
                return Err(AppError::Storage("no database".into()));
            }
            *self.value.lock().expect("lock") = Some(value.to_string());
            Ok(())
        }
    }

    fn notice(settings: FakeSettings) -> TrayNotice {
        TrayNotice::new(Arc::new(settings))
    }

    #[test]
    fn a_fresh_install_sees_the_notice() {
        assert!(notice(FakeSettings::default()).should_show());
    }

    #[test]
    fn dismissing_it_stops_it() {
        let notice = notice(FakeSettings::default());
        assert!(notice.should_show());

        notice.never_show_again();

        assert!(!notice.should_show());
    }

    /// The stored value is read, not merely checked for presence: a row
    /// saying "false" must not read as dismissed.
    #[test]
    fn a_false_value_still_shows_the_notice() {
        let settings = FakeSettings {
            value: Mutex::new(Some("false".to_string())),
            fail: false,
        };

        assert!(notice(settings).should_show());
    }

    #[test]
    fn an_unreadable_preference_shows_the_notice() {
        let settings = FakeSettings {
            value: Mutex::new(Some(TRUE.to_string())),
            fail: true,
        };

        assert!(notice(settings).should_show());
    }

    /// A failed write must not panic on an event-loop thread.
    #[test]
    fn a_failed_save_is_swallowed() {
        let settings = FakeSettings {
            value: Mutex::new(None),
            fail: true,
        };

        notice(settings).never_show_again();
    }
}
