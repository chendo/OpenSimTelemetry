//! Recursively flattens a `TelemetryFrame` (already-serialized as
//! `serde_json::Value`) into a flat map of dot-separated channel names →
//! values.
//!
//! ## Rules
//!
//! - Nested objects → keys joined with `.` (e.g. `motion.g_force.x`).
//! - Booleans → numeric `0` or `1`.
//! - Arrays of length ≤ 8 with scalar elements → expanded to `name.0`,
//!   `name.1`, etc. Larger arrays or arrays containing objects are
//!   skipped (kept out of the channel set entirely).
//! - Strings pass through unchanged.
//! - `null` is skipped.
//! - Non-finite numbers (`NaN`, `±Infinity`) cannot appear in
//!   `serde_json::Value` (the upstream serializer rejects them), so no
//!   special handling is needed here.
//!
//! Sparse vs dense semantics live one layer up in the format adapter —
//! this walker only reports what's present in *this* frame.

use serde_json::{Map, Value};

/// Walk `value` and populate `out` with one entry per leaf channel.
///
/// Caller is expected to clear `out` between frames if reusing the
/// allocation.
pub fn flatten_frame(value: &Value, out: &mut Map<String, Value>) {
    walk(value, "", out);
}

fn walk(value: &Value, prefix: &str, out: &mut Map<String, Value>) {
    match value {
        Value::Object(map) => {
            for (key, child) in map {
                let path = if prefix.is_empty() {
                    key.clone()
                } else {
                    format!("{}.{}", prefix, key)
                };
                walk(child, &path, out);
            }
        }
        Value::Array(items) => {
            // Skip oversized arrays — they're not channelized.
            if items.len() > 8 {
                return;
            }
            // Skip arrays containing nested objects/arrays — same reason.
            if items
                .iter()
                .any(|v| matches!(v, Value::Object(_) | Value::Array(_)))
            {
                return;
            }
            for (i, item) in items.iter().enumerate() {
                let path = format!("{}.{}", prefix, i);
                emit_leaf(&path, item, out);
            }
        }
        leaf => {
            if !prefix.is_empty() {
                emit_leaf(prefix, leaf, out);
            }
        }
    }
}

fn emit_leaf(path: &str, value: &Value, out: &mut Map<String, Value>) {
    match value {
        Value::Null => { /* skip */ }
        Value::Bool(b) => {
            out.insert(path.to_string(), Value::from(if *b { 1u8 } else { 0u8 }));
        }
        Value::Number(_) | Value::String(_) => {
            out.insert(path.to_string(), value.clone());
        }
        // Object/Array shouldn't reach here — walk() handles them — but
        // be defensive.
        Value::Object(_) | Value::Array(_) => { /* skip */ }
    }
}

/// Whether a flattened value is numeric (number or bool-as-number).
/// Used by the format adapter to decide which channels participate in
/// dense carry-forward.
pub fn is_numeric(value: &Value) -> bool {
    value.is_number()
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn flatten(v: Value) -> Map<String, Value> {
        let mut out = Map::new();
        flatten_frame(&v, &mut out);
        out
    }

    #[test]
    fn nested_objects_dot_join() {
        let out = flatten(json!({
            "vehicle": {"speed": 45.2, "rpm": 5000},
            "motion": {"g_force": {"x": 0.4, "y": -0.1, "z": 1.2}}
        }));
        assert_eq!(out.get("vehicle.speed"), Some(&json!(45.2)));
        assert_eq!(out.get("vehicle.rpm"), Some(&json!(5000)));
        assert_eq!(out.get("motion.g_force.x"), Some(&json!(0.4)));
        assert_eq!(out.get("motion.g_force.y"), Some(&json!(-0.1)));
        assert_eq!(out.get("motion.g_force.z"), Some(&json!(1.2)));
        assert!(out.values().all(|v| !v.is_object() && !v.is_array()));
    }

    #[test]
    fn booleans_become_numbers() {
        let out = flatten(json!({"vehicle": {"abs_active": true, "tc_active": false}}));
        assert_eq!(out.get("vehicle.abs_active"), Some(&json!(1)));
        assert_eq!(out.get("vehicle.tc_active"), Some(&json!(0)));
    }

    #[test]
    fn small_scalar_arrays_expand_with_index() {
        let out = flatten(json!({"iracing": {"tags": [1.0, 2.0, 3.0]}}));
        assert_eq!(out.get("iracing.tags.0"), Some(&json!(1.0)));
        assert_eq!(out.get("iracing.tags.1"), Some(&json!(2.0)));
        assert_eq!(out.get("iracing.tags.2"), Some(&json!(3.0)));
        assert!(out.get("iracing.tags").is_none());
    }

    #[test]
    fn arrays_at_threshold_still_expand() {
        let arr: Vec<Value> = (0..8).map(|i| json!(i)).collect();
        let out = flatten(json!({"x": arr}));
        for i in 0..8 {
            assert!(out.contains_key(&format!("x.{}", i)));
        }
    }

    #[test]
    fn arrays_over_eight_are_skipped() {
        let arr: Vec<Value> = (0..9).map(|i| json!(i)).collect();
        let out = flatten(json!({"x": arr}));
        assert!(out.is_empty(), "9-element array must be skipped entirely");
    }

    #[test]
    fn arrays_with_objects_are_skipped() {
        let out = flatten(json!({"competitors": [{"name": "A"}, {"name": "B"}]}));
        assert!(out.is_empty());
    }

    #[test]
    fn null_values_are_dropped() {
        let out = flatten(json!({"a": null, "b": 1}));
        assert!(out.get("a").is_none());
        assert_eq!(out.get("b"), Some(&json!(1)));
    }

    #[test]
    fn strings_pass_through() {
        let out = flatten(json!({"session": {"track_name": "Daytona"}}));
        assert_eq!(out.get("session.track_name"), Some(&json!("Daytona")));
    }

    #[test]
    fn empty_object_yields_nothing() {
        assert!(flatten(json!({})).is_empty());
    }

    #[test]
    fn is_numeric_classification() {
        assert!(is_numeric(&json!(1)));
        assert!(is_numeric(&json!(1.5)));
        assert!(is_numeric(&json!(0)));
        assert!(!is_numeric(&json!("hi")));
        assert!(!is_numeric(&json!(true)));
        assert!(!is_numeric(&json!(null)));
    }
}
