//! `cli algo diff`: the components that differ between two manifests, as dotted paths.

use serde_json::Value;

/// One differing leaf: its dotted path and the two values (`None` when absent on that side).
#[derive(Clone, Debug, PartialEq)]
pub struct Change {
    pub path: String,
    pub a: Option<Value>,
    pub b: Option<Value>,
}

/// Every leaf that differs between `a` and `b`, in path order. Arrays are compared whole.
pub fn diff(a: &Value, b: &Value) -> Vec<Change> {
    let mut out = Vec::new();
    walk("", Some(a), Some(b), &mut out);
    out
}

fn walk(path: &str, a: Option<&Value>, b: Option<&Value>, out: &mut Vec<Change>) {
    match (a, b) {
        (Some(Value::Object(x)), Some(Value::Object(y))) => {
            let mut keys: Vec<&String> = x.keys().chain(y.keys()).collect();
            keys.sort();
            keys.dedup();
            for k in keys {
                let p = if path.is_empty() {
                    k.clone()
                } else {
                    format!("{path}.{k}")
                };
                walk(&p, x.get(k), y.get(k), out);
            }
        }
        (x, y) if x == y => {}
        (x, y) => out.push(Change {
            path: path.to_owned(),
            a: x.cloned(),
            b: y.cloned(),
        }),
    }
}

/// The diff as text lines: `path: a -> b`.
pub fn render(changes: &[Change]) -> Vec<String> {
    let show = |v: &Option<Value>| v.as_ref().map_or("(absent)".to_owned(), Value::to_string);
    changes
        .iter()
        .map(|c| format!("{}: {} -> {}", c.path, show(&c.a), show(&c.b)))
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn it_lists_changed_added_and_removed_leaves() {
        let a = json!({ "facts": { "engine_nodes": 300000, "x": 1 }, "same": [1, 2] });
        let b = json!({ "facts": { "engine_nodes": 500000, "y": 2 }, "same": [1, 2] });
        let lines = render(&diff(&a, &b));
        assert_eq!(
            lines,
            vec![
                "facts.engine_nodes: 300000 -> 500000",
                "facts.x: 1 -> (absent)",
                "facts.y: (absent) -> 2",
            ]
        );
        assert!(diff(&a, &a).is_empty());
    }
}
