//! What a typed object needs to be written back the way it was read.
//!
//! Upstream never rebuilds a scene object on save: it holds the object
//! `JSON.parse` returned (spread into a new one by restore, which keeps the
//! property order, `restore.ts:500-508`), edits it by assigning properties
//! (`mutateElement.ts:80-100`), and writes it with `JSON.stringify`. So a
//! key keeps its place, a key the object lacked is appended when first
//! assigned, and a value nobody touched is written exactly as read.
//!
//! A typed struct loses some of that: its fields have a fixed order, and
//! reading can normalise a value (`customData: null` has no typed form; a
//! nested key the struct does not model is dropped). [`Layout`] records the
//! difference when an object is read, and [`Layout::write`] applies it:
//!
//! - the key order as read, when it differs from the canonical order; keys
//!   that are new since then follow in canonical order, then new unknown
//!   keys in insertion order;
//! - for each known key whose typed value would be written differently
//!   from the raw one (or that was absent and the typed model would add),
//!   the typed value at read time and the raw value. While the typed value
//!   is unchanged the raw one is written; once it changes, the typed one.
//!
//! Unknown keys are kept by the owner, in its `extra` map.

use serde_json::{Map, Value};
use std::collections::HashSet;

use crate::json::same;

/// Key order and read-time values of an object read from JSON; empty for an
/// object built in Rust.
#[derive(Debug, Clone, Default)]
pub(crate) struct Layout {
    /// Keys in the order read; `None` when that is the canonical order.
    order: Option<Vec<String>>,
    verbatim: Vec<Verbatim>,
}

/// A known key whose raw value differs from its typed form.
#[derive(Debug, Clone)]
struct Verbatim {
    key: String,
    /// The typed value when read; `None` if the model did not write the key.
    typed: Option<Value>,
    /// The raw value; `None` if the key was absent.
    raw: Option<Value>,
}

/// The canonical key order of an object: several key lists, in order.
pub(crate) type Canonical<'a> = &'a [&'a [&'a str]];

fn is_canonical(canonical: Canonical<'_>, key: &str) -> bool {
    canonical.iter().any(|keys| keys.contains(&key))
}

fn same_opt(a: Option<&Value>, b: Option<&Value>) -> bool {
    match (a, b) {
        (Some(a), Some(b)) => same(a, b),
        (None, None) => true,
        _ => false,
    }
}

impl Layout {
    /// Record how `raw` differs from `typed`, the typed model's view of the
    /// same object (the keys it would write, with their values). Returns
    /// the layout and the unknown keys: those of `raw` not in `canonical`.
    pub(crate) fn read(
        raw: &Map<String, Value>,
        typed: &Map<String, Value>,
        canonical: Canonical<'_>,
    ) -> (Layout, Map<String, Value>) {
        let mut extra = Map::new();
        let mut verbatim = Vec::new();
        for (key, value) in raw {
            if !is_canonical(canonical, key) && !typed.contains_key(key) {
                extra.insert(key.clone(), value.clone());
                continue;
            }
            let t = typed.get(key);
            if !same_opt(t, Some(value)) {
                verbatim.push(Verbatim {
                    key: key.clone(),
                    typed: t.cloned(),
                    raw: Some(value.clone()),
                });
            }
        }
        for (key, value) in typed {
            if !raw.contains_key(key) {
                verbatim.push(Verbatim {
                    key: key.clone(),
                    typed: Some(value.clone()),
                    raw: None,
                });
            }
        }
        let mut layout = Layout {
            order: None,
            verbatim,
        };
        let natural = layout.arrange(typed, &extra, canonical);
        if !natural
            .iter()
            .map(|(k, _)| *k)
            .eq(raw.keys().map(String::as_str))
        {
            layout.order = Some(raw.keys().cloned().collect());
        }
        (layout, extra)
    }

    /// The object to write: `typed` (the model's current keys and values)
    /// and `extra` (unknown keys) arranged as described in the module docs.
    /// An `extra` key the model owns (in `canonical` or `typed`) is skipped.
    pub(crate) fn write(
        &self,
        typed: &Map<String, Value>,
        extra: &Map<String, Value>,
        canonical: Canonical<'_>,
    ) -> Map<String, Value> {
        self.arrange(typed, extra, canonical)
            .into_iter()
            .map(|(k, v)| (k.to_owned(), v.clone()))
            .collect()
    }

    /// The value to write for a known key, or `None` to omit it.
    fn resolve<'a>(&'a self, key: &str, current: Option<&'a Value>) -> Option<&'a Value> {
        match self.verbatim.iter().find(|v| v.key == key) {
            Some(v) if same_opt(v.typed.as_ref(), current) => v.raw.as_ref(),
            _ => current,
        }
    }

    fn arrange<'a>(
        &'a self,
        typed: &'a Map<String, Value>,
        extra: &'a Map<String, Value>,
        canonical: Canonical<'a>,
    ) -> Vec<(&'a str, &'a Value)> {
        let known = |key: &str| is_canonical(canonical, key) || typed.contains_key(key);
        let mut out: Vec<(&'a str, &'a Value)> = Vec::with_capacity(typed.len() + extra.len());
        let mut seen: HashSet<&'a str> = HashSet::with_capacity(out.capacity());
        let mut emit = |key: &'a str, out: &mut Vec<(&'a str, &'a Value)>| {
            if !seen.insert(key) {
                return;
            }
            let value = if known(key) {
                self.resolve(key, typed.get(key))
            } else {
                extra.get(key)
            };
            if let Some(value) = value {
                out.push((key, value));
            }
        };
        if let Some(order) = &self.order {
            for key in order {
                emit(key, &mut out);
            }
        }
        for key in canonical.iter().flat_map(|keys| keys.iter()) {
            emit(key, &mut out);
        }
        for key in typed.keys() {
            emit(key, &mut out);
        }
        for key in extra.keys() {
            emit(key, &mut out);
        }
        out
    }
}
