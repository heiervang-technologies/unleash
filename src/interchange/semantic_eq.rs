use serde_json::Value;

/// Compare two JSON values for semantic equality.
/// Returns Ok(()) if equal, Err(path_description) if different.
pub fn semantic_eq(a: &Value, b: &Value) -> Result<(), String> {
    semantic_eq_inner(a, b, "$")
}

fn semantic_eq_inner(a: &Value, b: &Value, path: &str) -> Result<(), String> {
    match (a, b) {
        (Value::Null, Value::Null) => Ok(()),
        // An empty object and null are semantically equivalent for optional
        // fields like `extensions`, `metadata`, `project`, etc. This keeps
        // round-trip equality stable across converters that normalize "no
        // extra data" differently (some emit `{}`, others emit `null`).
        (Value::Null, Value::Object(m)) | (Value::Object(m), Value::Null) if m.is_empty() => {
            Ok(())
        }
        (Value::Bool(a), Value::Bool(b)) if a == b => Ok(()),
        (Value::Number(a), Value::Number(b)) => {
            let af = a.as_f64().unwrap_or(0.0);
            let bf = b.as_f64().unwrap_or(0.0);
            if (af - bf).abs() < 1e-6 || a == b {
                Ok(())
            } else {
                Err(format!("{path}: number mismatch: {a} != {b}"))
            }
        }
        (Value::String(a), Value::String(b)) => {
            if a == b {
                Ok(())
            } else {
                Err(format!("{path}: string mismatch: {a:?} != {b:?}"))
            }
        }
        (Value::Array(a), Value::Array(b)) => {
            if a.len() != b.len() {
                return Err(format!("{path}: array length {} != {}", a.len(), b.len()));
            }
            for (i, (av, bv)) in a.iter().zip(b.iter()).enumerate() {
                semantic_eq_inner(av, bv, &format!("{path}[{i}]"))?;
            }
            Ok(())
        }
        (Value::Object(a), Value::Object(b)) => {
            for (k, av) in a {
                let bv = b.get(k).unwrap_or(&Value::Null);
                if av == &Value::Null && bv == &Value::Null {
                    continue;
                }
                semantic_eq_inner(av, bv, &format!("{path}.{k}"))?;
            }
            for (k, bv) in b {
                if !a.contains_key(k) && bv != &Value::Null {
                    return Err(format!(
                        "{path}.{k}: missing in original, present in result"
                    ));
                }
            }
            Ok(())
        }
        _ => Err(format!(
            "{path}: type mismatch: {} != {}",
            type_name(a),
            type_name(b)
        )),
    }
}

fn type_name(v: &Value) -> &'static str {
    match v {
        Value::Null => "null",
        Value::Bool(_) => "bool",
        Value::Number(_) => "number",
        Value::String(_) => "string",
        Value::Array(_) => "array",
        Value::Object(_) => "object",
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn test_identical_values() {
        let v = json!({"a": 1, "b": "hello"});
        assert!(semantic_eq(&v, &v).is_ok());
    }

    #[test]
    fn test_key_order_irrelevant() {
        let a = json!({"a": 1, "b": 2});
        let b = json!({"b": 2, "a": 1});
        assert!(semantic_eq(&a, &b).is_ok());
    }

    #[test]
    fn test_null_vs_missing() {
        let a = json!({"a": 1, "b": null});
        let b = json!({"a": 1});
        assert!(semantic_eq(&a, &b).is_ok());
    }

    #[test]
    fn test_missing_vs_null() {
        let a = json!({"a": 1});
        let b = json!({"a": 1, "b": null});
        assert!(semantic_eq(&a, &b).is_ok());
    }

    #[test]
    fn test_float_precision() {
        let a = json!({"x": 1.0000001});
        let b = json!({"x": 1.0000002});
        assert!(semantic_eq(&a, &b).is_ok());
    }

    #[test]
    fn test_value_mismatch() {
        let a = json!({"a": 1});
        let b = json!({"a": 2});
        let err = semantic_eq(&a, &b).unwrap_err();
        assert!(err.contains("$.a"));
    }

    #[test]
    fn test_array_order_matters() {
        let a = json!([1, 2, 3]);
        let b = json!([1, 3, 2]);
        assert!(semantic_eq(&a, &b).is_err());
    }

    #[test]
    fn test_nested_objects() {
        let a = json!({"a": {"b": {"c": 42}}});
        let b = json!({"a": {"b": {"c": 42}}});
        assert!(semantic_eq(&a, &b).is_ok());
    }

    #[test]
    fn test_nested_mismatch_path() {
        let a = json!({"a": {"b": {"c": 42}}});
        let b = json!({"a": {"b": {"c": 99}}});
        let err = semantic_eq(&a, &b).unwrap_err();
        assert!(err.contains("$.a.b.c"));
    }

    #[test]
    fn test_empty_objects() {
        assert!(semantic_eq(&json!({}), &json!({})).is_ok());
    }

    #[test]
    fn test_empty_arrays() {
        assert!(semantic_eq(&json!([]), &json!([])).is_ok());
    }
}
