//! Whiteboards: one Yjs document each (`DocName::Whiteboard(id)`) holding
//! Excalidraw elements, plus an index in the Settings document
//! (`whiteboard:<id>` keys) so every member knows which boards exist.
//!
//! Document layout (see `polyph_proto::crdt`): `elements` Y.Map element id ->
//! JSON string of the Excalidraw element, `order` Y.Array of ids (z-order),
//! `meta` Y.Map (`name`). Concurrent edits to different elements merge; the
//! same element is last-writer-wins.

use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use yrs::{Any, Array, Doc, Map, Out, Transact};

use polyph_proto::crdt::whiteboard::{ELEMENTS, META, ORDER};

pub const INDEX_PREFIX: &str = "whiteboard:";
/// Elements one call may add.
pub const MAX_ELEMENTS_PER_CALL: usize = 500;

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct WhiteboardInfo {
    pub id: String,
    pub name: String,
    pub created_by: String,
    pub created_at: i64,
}

pub fn set_name(doc: &Doc, name: &str) {
    let meta = doc.get_or_insert_map(META);
    let mut txn = doc.transact_mut();
    meta.insert(&mut txn, "name", name);
}

/// Adds or replaces elements (each must have a string `id`). Returns their ids.
pub fn put(doc: &Doc, elements: &[Value]) -> Vec<String> {
    let map = doc.get_or_insert_map(ELEMENTS);
    let order = doc.get_or_insert_array(ORDER);
    let existing: Vec<String> = {
        let txn = doc.transact();
        order
            .iter(&txn)
            .filter_map(|o| match o {
                Out::Any(Any::String(s)) => Some(s.to_string()),
                _ => None,
            })
            .collect()
    };
    let mut txn = doc.transact_mut();
    let mut ids = vec![];
    for e in elements.iter().take(MAX_ELEMENTS_PER_CALL) {
        let Some(id) = e.get("id").and_then(|i| i.as_str()) else { continue };
        map.insert(&mut txn, id, e.to_string());
        if !existing.iter().any(|x| x == id) && !ids.iter().any(|x: &String| x == id) {
            order.push_back(&mut txn, id);
        }
        ids.push(id.to_string());
    }
    ids
}

pub fn remove(doc: &Doc, ids: &[String]) -> usize {
    let map = doc.get_or_insert_map(ELEMENTS);
    let order = doc.get_or_insert_array(ORDER);
    let mut txn = doc.transact_mut();
    let mut n = 0;
    for id in ids {
        if map.remove(&mut txn, id).is_some() {
            n += 1;
        }
    }
    // Drop removed ids from the order, last first so indexes stay valid.
    let positions: Vec<u32> = order
        .iter(&txn)
        .enumerate()
        .filter_map(|(i, o)| match o {
            Out::Any(Any::String(s)) if ids.iter().any(|x| x.as_str() == &*s) => Some(i as u32),
            _ => None,
        })
        .collect();
    for i in positions.into_iter().rev() {
        order.remove(&mut txn, i);
    }
    n
}

/// Elements in z-order (elements missing from the order come last).
pub fn elements(doc: &Doc) -> Vec<Value> {
    let map = doc.get_or_insert_map(ELEMENTS);
    let order = doc.get_or_insert_array(ORDER);
    let txn = doc.transact();
    let mut all: Vec<(String, Value)> = map
        .iter(&txn)
        .filter_map(|(k, o)| match o {
            Out::Any(Any::String(s)) => serde_json::from_str(&s).ok().map(|v| (k.to_string(), v)),
            _ => None,
        })
        .collect();
    let ord: Vec<String> = order
        .iter(&txn)
        .filter_map(|o| match o {
            Out::Any(Any::String(s)) => Some(s.to_string()),
            _ => None,
        })
        .collect();
    all.sort_by_key(|(k, _)| (ord.iter().position(|o| o == k).unwrap_or(usize::MAX), k.clone()));
    all.into_iter().map(|(_, v)| v).filter(|v| v.get("isDeleted") != Some(&json!(true))).collect()
}

/// A plain-text reading of a board for agents: shapes with their text,
/// positions, and arrows between labelled shapes.
pub fn describe(name: &str, els: &[Value]) -> String {
    let label = |e: &Value| -> String {
        let own = e.get("text").and_then(|t| t.as_str()).unwrap_or_default().to_string();
        if !own.is_empty() {
            return own;
        }
        // Text bound to a container.
        let id = e.get("id").and_then(|i| i.as_str()).unwrap_or_default();
        els.iter()
            .find(|t| t.get("containerId").and_then(|c| c.as_str()) == Some(id))
            .and_then(|t| t.get("text").and_then(|x| x.as_str()))
            .unwrap_or_default()
            .to_string()
    };
    let by_id = |id: &str| els.iter().find(|e| e.get("id").and_then(|i| i.as_str()) == Some(id));
    let mut s = format!("Whiteboard \"{name}\", {} elements.", els.len());
    for e in els {
        let kind = e.get("type").and_then(|t| t.as_str()).unwrap_or("element");
        if kind == "text" && e.get("containerId").and_then(|c| c.as_str()).is_some() {
            continue;
        }
        let id = e.get("id").and_then(|i| i.as_str()).unwrap_or_default();
        let (x, y) = (e["x"].as_f64().unwrap_or(0.0), e["y"].as_f64().unwrap_or(0.0));
        if kind == "arrow" || kind == "line" {
            let end = |k: &str| {
                e.get(k)
                    .and_then(|b| b.get("elementId"))
                    .and_then(|i| i.as_str())
                    .and_then(by_id)
                    .map(&label)
                    .filter(|l| !l.is_empty())
                    .unwrap_or_else(|| "(nothing)".into())
            };
            let l = label(e);
            let l = if l.is_empty() { String::new() } else { format!(" \"{l}\"") };
            s.push_str(&format!("\n- [{id}] {kind}{l} from {} to {}", end("startBinding"), end("endBinding")));
        } else {
            let l = label(e);
            let l = if l.is_empty() { String::new() } else { format!(" \"{l}\"") };
            s.push_str(&format!("\n- [{id}] {kind}{l} at ({x:.0}, {y:.0})"));
        }
    }
    s
}

/// Excalidraw elements for a simple diagram: labelled boxes and arrows.
/// `nodes`: (key, label); `edges`: (from key, to key, label). Laid out in a
/// grid from `origin`. Ids are `<prefix>-<key>` so redrawing replaces.
pub fn diagram(
    prefix: &str,
    nodes: &[(String, String)],
    edges: &[(String, String, String)],
    origin: (f64, f64),
    seed: u64,
) -> Vec<Value> {
    const W: f64 = 180.0;
    const H: f64 = 80.0;
    const GAP_X: f64 = 120.0;
    const GAP_Y: f64 = 100.0;
    let cols = (nodes.len() as f64).sqrt().ceil().max(1.0) as usize;
    let mut out = vec![];
    let pos = |i: usize| (origin.0 + (i % cols) as f64 * (W + GAP_X), origin.1 + (i / cols) as f64 * (H + GAP_Y));
    let base = |id: String, kind: &str, x: f64, y: f64, w: f64, h: f64, n: u64| {
        json!({
            "id": id, "type": kind, "x": x, "y": y, "width": w, "height": h, "angle": 0,
            "strokeColor": "#1e1e1e", "backgroundColor": "transparent", "fillStyle": "solid",
            "strokeWidth": 2, "strokeStyle": "solid", "roughness": 1, "opacity": 100,
            "groupIds": [], "frameId": null, "roundness": null, "seed": seed.wrapping_add(n),
            "version": 1, "versionNonce": seed.wrapping_mul(31).wrapping_add(n), "isDeleted": false,
            "boundElements": [], "updated": 0, "link": null, "locked": false
        })
    };
    for (i, (key, text)) in nodes.iter().enumerate() {
        let (x, y) = pos(i);
        let id = format!("{prefix}-{key}");
        let tid = format!("{id}-text");
        let mut rect = base(id.clone(), "rectangle", x, y, W, H, i as u64 * 2);
        rect["roundness"] = json!({ "type": 3 });
        let mut bound = vec![json!({ "type": "text", "id": tid })];
        for (j, (f, t, _)) in edges.iter().enumerate() {
            if f == key || t == key {
                bound.push(json!({ "type": "arrow", "id": format!("{prefix}-e{j}") }));
            }
        }
        rect["boundElements"] = json!(bound);
        let mut txt = base(tid, "text", x + 10.0, y + H / 2.0 - 12.0, W - 20.0, 24.0, i as u64 * 2 + 1);
        txt["text"] = json!(text);
        txt["originalText"] = json!(text);
        txt["fontSize"] = json!(20);
        txt["fontFamily"] = json!(1);
        txt["textAlign"] = json!("center");
        txt["verticalAlign"] = json!("middle");
        txt["containerId"] = json!(id);
        txt["lineHeight"] = json!(1.25);
        out.push(rect);
        out.push(txt);
    }
    for (j, (f, t, text)) in edges.iter().enumerate() {
        let (Some(a), Some(b)) = (nodes.iter().position(|n| &n.0 == f), nodes.iter().position(|n| &n.0 == t)) else {
            continue;
        };
        let (pa, pb) = (pos(a), pos(b));
        let (sx, sy) = (pa.0 + W / 2.0, pa.1 + H / 2.0);
        let (ex, ey) = (pb.0 + W / 2.0, pb.1 + H / 2.0);
        let mut arr = base(format!("{prefix}-e{j}"), "arrow", sx, sy, ex - sx, ey - sy, 10_000 + j as u64);
        arr["points"] = json!([[0, 0], [ex - sx, ey - sy]]);
        arr["startBinding"] = json!({ "elementId": format!("{prefix}-{f}"), "focus": 0, "gap": 4 });
        arr["endBinding"] = json!({ "elementId": format!("{prefix}-{t}"), "focus": 0, "gap": 4 });
        arr["startArrowhead"] = Value::Null;
        arr["endArrowhead"] = json!("arrow");
        if !text.is_empty() {
            arr["text"] = json!(text);
        }
        out.push(arr);
    }
    out
}

#[cfg(test)]
mod tests {
    #![allow(clippy::unwrap_used)]
    use super::*;

    #[test]
    fn put_describe_remove() {
        let d = Doc::new();
        set_name(&d, "Architecture");
        let els = diagram(
            "arch",
            &[("api".into(), "API".into()), ("db".into(), "Postgres".into())],
            &[("api".into(), "db".into(), "reads".into())],
            (0.0, 0.0),
            7,
        );
        assert_eq!(els.len(), 5);
        put(&d, &els);
        let got = elements(&d);
        assert_eq!(got.len(), 5);
        let text = describe("Architecture", &got);
        assert!(text.contains("rectangle \"API\" at (0, 0)"), "{text}");
        assert!(text.contains("arrow \"reads\" from API to Postgres"), "{text}");
        // Redrawing replaces instead of duplicating.
        put(&d, &els);
        assert_eq!(elements(&d).len(), 5);
        assert_eq!(remove(&d, &["arch-e0".into()]), 1);
        assert_eq!(elements(&d).len(), 4);
    }
}
