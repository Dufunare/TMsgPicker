use serde::Serialize;

/// Navigation and account context are independent. No target means no account.
#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub struct SettingsTarget {
    pub tab: &'static str,
    pub profile_id: Option<String>,
}

impl Default for SettingsTarget {
    fn default() -> Self {
        Self {
            tab: "application",
            profile_id: None,
        }
    }
}

impl SettingsTarget {
    pub fn resolve(
        tab: Option<&str>,
        profile_id: Option<String>,
        validate: impl FnOnce(&str) -> Result<(), String>,
    ) -> Result<Self, String> {
        let tab = match tab {
            Some("application") => "application",
            Some("account") => "account",
            Some("appearance") => "appearance",
            Some(_) => return Err("Unknown settings tab".into()),
            // Compatibility with callers predating the independent tab argument.
            None if profile_id.is_some() => "account",
            None => "application",
        };
        if let Some(id) = &profile_id {
            if id.is_empty() {
                return Err("Unknown linked account".into());
            }
            validate(id)?;
        }
        Ok(Self { tab, profile_id })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn linked(id: &str) -> Result<(), String> {
        if ["A", "B"].contains(&id) {
            Ok(())
        } else {
            Err("Unknown linked account".into())
        }
    }

    #[test]
    fn explicit_tabs_keep_the_same_account_and_legacy_calls_work() {
        for tab in ["application", "account", "appearance"] {
            let target = SettingsTarget::resolve(Some(tab), Some("B".into()), linked).unwrap();
            assert_eq!(target.tab, tab);
            assert_eq!(target.profile_id.as_deref(), Some("B"));
        }
        assert_eq!(
            SettingsTarget::resolve(None, Some("B".into()), linked)
                .unwrap()
                .tab,
            "account"
        );
        assert_eq!(
            SettingsTarget::resolve(None, None, linked).unwrap(),
            SettingsTarget::default()
        );
        assert_eq!(
            SettingsTarget::resolve(Some("account"), None, linked)
                .unwrap()
                .profile_id,
            None
        );
    }

    #[test]
    fn missing_accounts_and_invalid_tabs_never_fall_back() {
        for id in ["", "deleted-B"] {
            assert!(SettingsTarget::resolve(Some("application"), Some(id.into()), linked).is_err());
        }
        assert!(SettingsTarget::resolve(Some("invalid"), Some("A".into()), linked).is_err());
        assert!(
            SettingsTarget::resolve(Some("account"), Some("B".into()), |_| Err("deleted".into()))
                .is_err()
        );
    }
}
