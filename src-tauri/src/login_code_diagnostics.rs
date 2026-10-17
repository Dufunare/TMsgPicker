//! Privacy-minimal diagnostic projection for Telegram login-code delivery.
//! Never put a raw TDLib authenticationCodeInfo on a UI event: it includes the
//! phone number and future schema versions may contain additional credentials.

use serde::Serialize;
use serde_json::Value;

#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub(crate) struct LoginCodeDiagnostics {
    pub code_info_received: bool,
    pub code_type: Option<String>,
    pub next_code_type: Option<String>,
    pub timeout_seconds: Option<i64>,
    pub length: Option<i64>,
}

fn safe_code_type(value: &Value) -> Option<String> {
    let name = value.get("@type")?.as_str()?;
    // Exact allowlist: arbitrary strings and unexpected future TDLib fields
    // must not be reflected into the WebView or a screenshot.
    let known = matches!(
        name,
        "authenticationCodeTypeTelegramMessage"
            | "authenticationCodeTypeSms"
            | "authenticationCodeTypeSmsWord"
            | "authenticationCodeTypeSmsPhrase"
            | "authenticationCodeTypeCall"
            | "authenticationCodeTypeFlashCall"
            | "authenticationCodeTypeMissedCall"
            | "authenticationCodeTypeFragment"
            | "authenticationCodeTypeFirebaseAndroid"
            | "authenticationCodeTypeFirebaseIos"
            | "authenticationCodeTypeEmailCode"
            | "authenticationCodeTypeSetUpEmail"
            | "authenticationCodeTypeGoogleId"
            | "authenticationCodeTypeAppleId"
    );
    Some(if known { name } else { "unrecognized" }.to_string())
}

impl LoginCodeDiagnostics {
    pub(crate) fn from_wait_code(authorization_state: &Value) -> Self {
        let info = &authorization_state["code_info"];
        if info["@type"] != "authenticationCodeInfo" {
            return Self {
                code_info_received: false,
                code_type: None,
                next_code_type: None,
                timeout_seconds: None,
                length: None,
            };
        }

        // Only project these four non-sensitive TDLib fields. In particular,
        // ignore code_info.phone_number and ALL nested extra values.
        let kind = &info["type"];
        Self {
            code_info_received: true,
            code_type: safe_code_type(kind),
            next_code_type: safe_code_type(&info["next_type"]),
            timeout_seconds: info["timeout"].as_i64().filter(|v| *v >= 0),
            length: kind["length"].as_i64().filter(|v| *v > 0 && *v <= 32),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn telegram_app_delivery_includes_channel_and_timeout_but_no_secrets() {
        let raw = json!({
            "@type":"authorizationStateWaitCode",
            "code_info":{
                "@type":"authenticationCodeInfo",
                "phone_number":"+810000000000",
                "phone_code_hash":"do-not-expose",
                "type":{
                    "@type":"authenticationCodeTypeTelegramMessage",
                    "length":5,
                    "future_secret":"nested-do-not-expose"
                },
                "next_type":{"@type":"authenticationCodeTypeSms","length":6},
                "timeout":42
            }
        });
        let diagnostic = LoginCodeDiagnostics::from_wait_code(&raw);
        assert_eq!(diagnostic.code_type.as_deref(), Some("authenticationCodeTypeTelegramMessage"));
        assert_eq!(diagnostic.next_code_type.as_deref(), Some("authenticationCodeTypeSms"));
        assert_eq!(diagnostic.timeout_seconds, Some(42));
        assert_eq!(diagnostic.length, Some(5));
        let serialized = serde_json::to_string(&diagnostic).unwrap();
        assert!(!serialized.contains("+810000000000"));
        assert!(!serialized.contains("do-not-expose"));
        assert!(!serialized.contains("phone_number"));
        assert!(diagnostic.code_info_received);
    }

    #[test]
    fn absent_or_null_code_info_is_explicitly_reported() {
        for raw in [
            json!({"@type":"authorizationStateWaitCode"}),
            json!({"@type":"authorizationStateWaitCode","code_info":null}),
        ] {
            let diagnostic = LoginCodeDiagnostics::from_wait_code(&raw);
            assert!(!diagnostic.code_info_received);
            assert_eq!(diagnostic.code_type, None);
            assert_eq!(diagnostic.timeout_seconds, None);
        }
    }

    #[test]
    fn no_next_channel_is_not_misreported_as_sms() {
        let raw = json!({"code_info":{
            "@type":"authenticationCodeInfo",
            "type":{"@type":"authenticationCodeTypeTelegramMessage","length":0},
            "next_type":null,
            "timeout":0
        }});
        let diagnostic = LoginCodeDiagnostics::from_wait_code(&raw);
        assert!(diagnostic.code_info_received);
        assert_eq!(diagnostic.next_code_type, None);
        assert_eq!(diagnostic.length, None);
        assert_eq!(diagnostic.timeout_seconds, Some(0));
    }

    #[test]
    fn unknown_channel_names_and_arbitrary_values_are_not_forwarded() {
        let raw = json!({"code_info":{
            "@type":"authenticationCodeInfo",
            "phone_number":"+123",
            "type":{"@type":"unexpected-private-data-123","length":-1},
            "next_type":{"@type":"injected-user-data"},
            "timeout":-7
        }});
        let diagnostic = LoginCodeDiagnostics::from_wait_code(&raw);
        assert_eq!(diagnostic.code_type.as_deref(), Some("unrecognized"));
        assert_eq!(diagnostic.next_code_type.as_deref(), Some("unrecognized"));
        assert_eq!(diagnostic.length, None);
        assert_eq!(diagnostic.timeout_seconds, None);
        let json = serde_json::to_string(&diagnostic).unwrap();
        assert!(!json.contains("+123"));
        assert!(!json.contains("injected"));
    }
}
