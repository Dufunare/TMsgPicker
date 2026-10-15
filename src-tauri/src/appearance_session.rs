use crate::{gateway::Result, settings::AppearancePreferences};
use serde::Serialize;

#[derive(Clone, Serialize)]
pub(super) struct AppearanceSnapshot {
    pub appearance: AppearancePreferences,
    pub revision: u64,
    pub editor: u64,
}

/// Preview lives only in this process; Store remains the committed source of truth.
#[derive(Default)]
pub(super) struct AppearanceSession {
    preview: Option<AppearancePreferences>,
    revision: u64,
    editor: u64,
}
impl AppearanceSession {
    pub fn snapshot(&self, saved: AppearancePreferences) -> AppearanceSnapshot {
        AppearanceSnapshot {
            appearance: self.preview.clone().unwrap_or(saved),
            revision: self.revision,
            editor: self.editor,
        }
    }
    pub fn update(
        &mut self,
        editor: u64,
        next: AppearancePreferences,
        persist: Option<impl FnOnce(&AppearancePreferences) -> Result<()>>,
    ) -> Result<()> {
        if editor != self.editor {
            return Err("外观设置窗口已关闭，请重新打开设置".into());
        }
        next.validate()?;
        if let Some(persist) = persist {
            persist(&next)?;
            self.preview = None;
        } else {
            self.preview = Some(next);
        }
        self.revision += 1;
        Ok(())
    }
    pub fn cancel(&mut self) {
        self.preview = None;
        self.editor += 1; // Invalidate commands still in flight from the destroyed editor.
        self.revision += 1;
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    type Persist = fn(&AppearancePreferences) -> Result<()>;
    #[test]
    fn preview_cancel_and_reopen_do_not_commit_or_accept_stale_editors() {
        let saved = AppearancePreferences::default();
        let mut session = AppearanceSession::default();
        let glass = AppearancePreferences {
            theme: "glass".into(),
            ..saved.clone()
        };
        session.update(0, glass.clone(), None::<Persist>).unwrap();
        assert_eq!(session.snapshot(saved.clone()).appearance, glass);
        session.cancel();
        assert_eq!(session.snapshot(saved.clone()).appearance, saved);
        assert!(session.update(0, glass, None::<Persist>).is_err());
        assert_eq!(session.snapshot(saved).editor, 1);
    }
    #[test]
    fn only_successful_commit_changes_saved_appearance_and_cancel_preserves_it() {
        let mut saved = AppearancePreferences::default();
        let mut session = AppearanceSession::default();
        let dark = AppearancePreferences {
            theme: "dark".into(),
            ..saved.clone()
        };
        session.update(0, dark.clone(), None::<Persist>).unwrap();
        let revision = session.revision;
        assert!(
            session
                .update(
                    0,
                    dark.clone(),
                    Some(|_: &AppearancePreferences| Err("synthetic write failure".into()))
                )
                .is_err()
        );
        assert_eq!(session.revision, revision);
        assert_eq!(saved.theme, "system");
        assert_eq!(session.snapshot(saved.clone()).appearance, dark);
        session
            .update(
                0,
                dark.clone(),
                Some(|next: &AppearancePreferences| {
                    saved = next.clone();
                    Ok(())
                }),
            )
            .unwrap();
        let glass = AppearancePreferences {
            theme: "glass".into(),
            ..saved.clone()
        };
        session.update(0, glass, None::<Persist>).unwrap();
        session.cancel();
        assert_eq!(session.snapshot(saved).appearance, dark);
        assert_eq!(
            AppearanceSession::default()
                .snapshot(dark.clone())
                .appearance,
            dark
        );
    }
}
