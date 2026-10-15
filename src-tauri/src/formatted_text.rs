//! TDLib formattedText link extraction, separate from Post grouping and UI.
use serde::{Deserialize, Serialize};
use serde_json::Value;

/// Offsets and lengths use UTF-16 code units, as defined by TDLib. JavaScript
/// String.slice uses the same units, including for non-BMP emoji.
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
pub struct TextLink {
    pub offset: usize,
    pub length: usize,
    pub url: String,
}

fn number(value: &Value) -> Option<i64> {
    value.as_i64().or_else(|| value.as_str()?.parse().ok())
}

/// Convert a UTF-16 offset to an exact UTF-8 boundary. Reject malformed
/// offsets inside a surrogate pair instead of generating broken link text.
fn utf16_byte_boundary(text: &str, target: usize) -> Option<usize> {
    let mut units = 0usize;
    for (byte, ch) in text.char_indices() {
        if units == target {
            return Some(byte);
        }
        units = units.checked_add(ch.len_utf16())?;
        if units > target {
            return None;
        }
    }
    (units == target).then_some(text.len())
}

/// Preserve TDLib's textEntityTypeTextUrl and textEntityTypeUrl entities.
/// Formatting-only entities (bold, italic, etc.) are not navigation targets.
/// The renderer independently enforces a strict HTTP(S)-only URL policy.
pub fn extract_text_links(formatted: &Value) -> Vec<TextLink> {
    let Some(text) = formatted["text"].as_str() else {
        return Vec::new();
    };
    let Some(entities) = formatted["entities"].as_array() else {
        return Vec::new();
    };
    entities
        .iter()
        .filter_map(|entity| {
            let offset = usize::try_from(number(&entity["offset"])?).ok()?;
            let length = usize::try_from(number(&entity["length"])?).ok()?;
            if length == 0 {
                return None;
            }
            let end = offset.checked_add(length)?;
            let from = utf16_byte_boundary(text, offset)?;
            let to = utf16_byte_boundary(text, end)?;
            let url = match entity["type"]["@type"].as_str()? {
                "textEntityTypeTextUrl" => entity["type"]["url"].as_str()?.to_owned(),
                "textEntityTypeUrl" => {
                    let visible = &text[from..to];
                    let lower = visible.to_ascii_lowercase();
                    if lower.starts_with("http://") || lower.starts_with("https://") {
                        visible.to_owned()
                    } else if visible.contains('.')
                        && !visible.contains("://")
                        && !visible.chars().any(char::is_whitespace)
                    {
                        format!("https://{visible}")
                    } else {
                        return None;
                    }
                }
                _ => return None,
            };
            Some(TextLink {
                offset,
                length,
                url,
            })
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn finds_hidden_and_visible_urls_with_unicode_utf16_indices() {
        let links = extract_text_links(&json!({
            "text": "🧭 点击这里，再看 t.me/demo",
            "entities": [
                {"offset": 3, "length": 4,
                 "type": {"@type": "textEntityTypeTextUrl",
                          "url": "https://example.org/target?x=1"}},
                {"offset": 11, "length": 9,
                 "type": {"@type": "textEntityTypeUrl"}},
                {"offset": 3, "length": 4,
                 "type": {"@type": "textEntityTypeBold"}}
            ]
        }));
        assert_eq!(
            links,
            vec![
                TextLink {
                    offset: 3,
                    length: 4,
                    url: "https://example.org/target?x=1".into()
                },
                TextLink {
                    offset: 11,
                    length: 9,
                    url: "https://t.me/demo".into()
                }
            ]
        );
    }

    #[test]
    fn rejects_invalid_or_surrogate_splitting_ranges_without_panicking() {
        let links = extract_text_links(&json!({
            "text": "😀Hello",
            "entities": [
                {"offset": 1, "length": 2,
                 "type": {"@type": "textEntityTypeTextUrl", "url": "https://bad.example"}},
                {"offset": -1, "length": 2,
                 "type": {"@type": "textEntityTypeTextUrl", "url": "https://bad.example"}},
                {"offset": 9999, "length": 2,
                 "type": {"@type": "textEntityTypeTextUrl", "url": "https://bad.example"}},
                {"offset": 2, "length": 5,
                 "type": {"@type": "textEntityTypeTextUrl", "url": "https://good.example"}}
            ]
        }));
        assert_eq!(
            links,
            vec![TextLink {
                offset: 2,
                length: 5,
                url: "https://good.example".into()
            }]
        );
        assert!(extract_text_links(&json!({"text": "plain"})).is_empty());
    }
}
