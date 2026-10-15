use serde_json::{Value, json};
use tmsgpicker_lib::core::{assemble, normalize};

fn raw(id: i64, content: Value, album: i64) -> Value {
    json!({
        "id": id,
        "chat_id": -1001,
        "date": id,
        "media_album_id": album,
        "content": content,
        "interaction_info": null
    })
}

#[test]
fn hidden_text_link_survives_normalization_and_post_grouping() {
    let mut first = raw(
        100,
        json!({
            "@type": "messagePhoto",
            "caption": {
                "@type": "formattedText",
                "text": "🧭 点击这里",
                "entities": [
                    {"offset": 3, "length": 4, "type": {
                        "@type": "textEntityTypeTextUrl",
                        "url": "https://example.org/linked-text"
                    }}
                ]
            },
            "photo": {"sizes": []}
        }),
        9001,
    );
    let second = raw(
        101,
        json!({"@type": "messageVideo", "caption": {"text": ""}}),
        9001,
    );
    let first_msg = normalize(&first).unwrap();
    assert_eq!(first_msg.text, "🧭 点击这里");
    assert_eq!(first_msg.links.len(), 1);
    assert_eq!(first_msg.links[0].offset, 3);
    assert_eq!(first_msg.links[0].length, 4);

    let posts = assemble(&[first_msg, normalize(&second).unwrap()]);
    assert_eq!(posts.len(), 1);
    let post = &posts[0];
    assert_eq!(post.text, "🧭 点击这里");
    assert_eq!(post.links.len(), 1);
    assert_eq!(post.links[0].url, "https://example.org/linked-text");
    assert_eq!(post.members[0].links, post.links);
    let result = serde_json::to_value(post).unwrap();
    assert_eq!(result["links"][0]["offset"], 3);
    assert_eq!(result["links"][0]["url"], "https://example.org/linked-text");

    // A normal text message has the same entity representation.
    first["content"] = json!({
        "@type": "messageText",
        "text": {
            "text": "打开这里",
            "entities": [{"offset": 0, "length": 4, "type": {
                "@type": "textEntityTypeTextUrl",
                "url": "https://example.org/text-message"
            }}]
        }
    });
    first["media_album_id"] = json!(0);
    let text_msg = normalize(&first).unwrap();
    assert_eq!(text_msg.links[0].url, "https://example.org/text-message");
    assert_eq!(assemble(&[text_msg])[0].links.len(), 1);
}

#[test]
fn missing_entities_and_older_payloads_stay_compatible() {
    let plain = normalize(&raw(
        105,
        json!({"@type": "messageText", "text": {"text": "Hello"}}),
        0,
    ))
    .unwrap();
    assert!(plain.links.is_empty());
    let mut json = serde_json::to_value(&plain).unwrap();
    json.as_object_mut().unwrap().remove("links");
    let restored: tmsgpicker_lib::core::TelegramMessage = serde_json::from_value(json).unwrap();
    assert!(restored.links.is_empty());
}
