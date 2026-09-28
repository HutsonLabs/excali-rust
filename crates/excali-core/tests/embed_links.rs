//! `getEmbedLink` (`packages/element/src/embeddable.ts:171-400`) against
//! upstream's own answers at the pinned commit
//! (`tests/fixtures/embed-links.json`, written by
//! `tools/goldens/svg-export.mjs`): the link an embeddable's `<iframe>`
//! loads, the kind of embed and its intrinsic size, for links of every
//! rule upstream has and for links that fall through them.

use excali_core::embeddable::{get_embed_link, EmbedType};
use serde_json::Value;

#[test]
fn every_link_embeds_as_upstream_embeds_it() {
    let fixture: Value = serde_json::from_str(include_str!("fixtures/embed-links.json")).unwrap();
    let links = fixture["links"].as_array().unwrap();
    assert!(links.len() >= 40);
    let mut failures = Vec::new();
    for case in links {
        let link = case["link"].as_str().unwrap();
        let got = get_embed_link(link).map(|r| {
            let kind = match r.kind {
                EmbedType::Video => "video",
                EmbedType::Generic => "generic",
                EmbedType::Document => "document",
            };
            serde_json::json!({
                "link": r.link,
                "type": kind,
                "intrinsicSize": { "w": r.intrinsic_size[0], "h": r.intrinsic_size[1] },
            })
        });
        let expected = &case["result"];
        let got = got.unwrap_or(Value::Null);
        // intrinsic sizes are whole numbers: compare them as numbers
        if normalize(&got) != normalize(expected) {
            failures.push(format!("{link}:\n  got      {got}\n  expected {expected}"));
        }
    }
    assert!(failures.is_empty(), "{}", failures.join("\n"));
}

fn normalize(v: &Value) -> Value {
    match v {
        Value::Number(n) => serde_json::json!(n.as_f64().unwrap()),
        Value::Object(o) => {
            Value::Object(o.iter().map(|(k, v)| (k.clone(), normalize(v))).collect())
        }
        other => other.clone(),
    }
}

#[test]
fn an_empty_link_embeds_nothing() {
    assert_eq!(get_embed_link(""), None);
}
