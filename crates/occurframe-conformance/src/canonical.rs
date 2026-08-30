use sha2::{Digest, Sha256};

use crate::Result;

/// Serialize JSON with stable object-key ordering and no locale/path/time input.
pub fn canonical_json<T: serde::Serialize>(value: &T) -> Result<Vec<u8>> {
    let value = serde_json::to_value(value)?;
    let sorted = sort_value(value);
    Ok(serde_json::to_vec(&sorted)?)
}

/// Serialize one canonical JSON record terminated by exactly one LF byte.
pub fn canonical_json_line<T: serde::Serialize>(value: &T) -> Result<Vec<u8>> {
    let mut bytes = canonical_json(value)?;
    bytes.push(b'\n');
    Ok(bytes)
}

/// Serialize indented JSON with sorted keys, UTF-8, and exactly one trailing LF.
pub fn canonical_pretty_json<T: serde::Serialize>(value: &T) -> Result<Vec<u8>> {
    let value = serde_json::to_value(value)?;
    let sorted = sort_value(value);
    let mut bytes = serde_json::to_vec_pretty(&sorted)?;
    bytes.push(b'\n');
    Ok(bytes)
}

/// Compute a lowercase SHA-256 digest.
#[must_use]
pub fn sha256_hex(bytes: &[u8]) -> String {
    hex::encode(Sha256::digest(bytes))
}

fn sort_value(value: serde_json::Value) -> serde_json::Value {
    match value {
        serde_json::Value::Object(object) => {
            let mut entries: Vec<_> = object.into_iter().collect();
            entries.sort_by(|left, right| left.0.cmp(&right.0));
            let object = entries
                .into_iter()
                .map(|(key, value)| (key, sort_value(value)))
                .collect();
            serde_json::Value::Object(object)
        }
        serde_json::Value::Array(values) => {
            serde_json::Value::Array(values.into_iter().map(sort_value).collect())
        }
        scalar => scalar,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn keys_sort_but_arrays_do_not() {
        let input = serde_json::json!({"z": ["b", "a", "a"], "a": {"y": 1, "x": 2}});
        let output = String::from_utf8(canonical_json(&input).expect("canonical JSON"))
            .expect("UTF-8 output");
        assert_eq!(output, r#"{"a":{"x":2,"y":1},"z":["b","a","a"]}"#);
    }
}
