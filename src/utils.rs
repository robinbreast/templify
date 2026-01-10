//! Utility functions for the templify engine.
//!
//! This module provides common helper functions used throughout the library and CLI:
//! - Environment variable expansion (Unix `${VAR}` and Windows `%VAR%` syntax)
//! - File format detection based on extension or explicit hint
//! - Nested JSON value insertion using dot-notation keys

use regex::Regex;
use serde_json::Value;
use std::collections::HashMap;
use std::env;
use std::path::Path;

/// Expands environment variables in a string.
///
/// Supports both Unix-style `${VAR}` and Windows-style `%VAR%` syntax.
/// If a variable is not found, the original token is preserved.
pub fn expand_env_vars(value: &str) -> String {
    let unix_re = Regex::new(r"\$\{([A-Za-z0-9_]+)\}").unwrap();
    let windows_re = Regex::new(r"%([A-Za-z0-9_]+)%").unwrap();

    let unix_expanded = unix_re.replace_all(value, |caps: &regex::Captures<'_>| {
        env::var(&caps[1]).unwrap_or_else(|_| caps[0].to_string())
    });

    windows_re
        .replace_all(&unix_expanded, |caps: &regex::Captures<'_>| {
            env::var(&caps[1]).unwrap_or_else(|_| caps[0].to_string())
        })
        .to_string()
}

/// Detects the file format based on a hint or file extension.
///
/// Returns `"json"`, `"yaml"`, or `"toml"`. Defaults to `"json"` if unrecognized.
pub fn detect_format(format: &str, path: &Path) -> String {
    if format != "auto" {
        return format.to_lowercase();
    }
    match path
        .extension()
        .and_then(|s| s.to_str())
        .unwrap_or("")
        .to_lowercase()
        .as_str()
    {
        "yml" | "yaml" => "yaml".to_string(),
        "toml" => "toml".to_string(),
        _ => "json".to_string(),
    }
}

/// Inserts a value into a HashMap using dot-notation keys.
///
/// Creates nested JSON objects as needed. For example, `"utils.string"` creates
/// `{"utils": {"string": <value>}}`.
pub fn insert_nested_value(target: &mut HashMap<String, Value>, key: &str, value: Value) {
    let mut parts = key.split('.');
    let Some(root) = parts.next() else {
        return;
    };
    let rest: Vec<&str> = parts.collect();
    if rest.is_empty() {
        target.insert(root.to_string(), value);
        return;
    }

    let entry = target
        .entry(root.to_string())
        .or_insert_with(|| Value::Object(serde_json::Map::new()));
    insert_nested_value_in(entry, &rest, value);
}

fn insert_nested_value_in(target: &mut Value, parts: &[&str], value: Value) {
    if parts.is_empty() {
        *target = value;
        return;
    }

    if let Value::Object(map) = target {
        if parts.len() == 1 {
            map.insert(parts[0].to_string(), value);
            return;
        }
        let entry = map
            .entry(parts[0].to_string())
            .or_insert_with(|| Value::Object(serde_json::Map::new()));
        insert_nested_value_in(entry, &parts[1..], value);
    } else {
        *target = Value::Object(serde_json::Map::new());
        insert_nested_value_in(target, parts, value);
    }
}
