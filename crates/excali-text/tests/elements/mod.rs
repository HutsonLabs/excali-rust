//! Elements as upstream's `API.createElement`
//! (`packages/excalidraw/tests/helpers/api.ts:166-436`) builds them in its
//! test mode, for the fields the bound-text tests read.

#![allow(dead_code)]

use excali_core::element::Element;
use serde_json::{json, Map, Value};

fn obj(v: Value) -> Map<String, Value> {
    match v {
        Value::Object(m) => m,
        _ => panic!("not an object"),
    }
}

/// The element as a JSON object: the shared defaults, the type's own,
/// then `fields`.
pub fn element_map(ty: &str, id: &str, fields: Value) -> Map<String, Value> {
    let mut element = obj(json!({
        "id": id,
        "type": ty,
        "x": 0,
        "y": 0,
        "width": 100,
        "height": 100,
        "angle": 0,
        "strokeColor": "#1e1e1e",
        "backgroundColor": "transparent",
        "fillStyle": "solid",
        "strokeWidth": 2,
        "strokeStyle": "solid",
        "roughness": 1,
        "opacity": 100,
        "groupIds": [],
        "frameId": null,
        "index": null,
        "roundness": null,
        "seed": 1,
        "version": 1,
        "versionNonce": 0,
        "isDeleted": false,
        "boundElements": null,
        "updated": 1,
        "created": 1,
        "link": null,
        "locked": false
    }));
    match ty {
        "text" => element.extend(obj(json!({
            "text": "test",
            "fontSize": 20,
            "baseFontSize": null,
            "fontFamily": 5,
            "textAlign": "left",
            "verticalAlign": "top",
            "containerId": null,
            "originalText": "test",
            "autoResize": true,
            "lineHeight": 1.25,
            "labelPosition": null
        }))),
        "arrow" | "line" => {
            element.extend(obj(json!({
                "points": [[0, 0], [100, 100]],
                "startBinding": null,
                "endBinding": null,
                "startArrowhead": null,
                "endArrowhead": null
            })));
            if ty == "arrow" {
                element.insert("elbowed".into(), json!(false));
            } else {
                element.insert("polygon".into(), json!(false));
            }
        }
        "stickynote" => element.extend(obj(json!({
            "width": 250,
            "height": 250,
            "baseHeight": 250,
            "backgroundColor": "#ffdf6b",
            "roundness": { "type": 3 }
        }))),
        _ => {}
    }
    element.extend(obj(fields));
    element
}

/// [`element_map`] read into the element model.
pub fn element(ty: &str, id: &str, fields: Value) -> Element {
    Element::from_map(element_map(ty, id, fields)).expect("a valid element")
}
