use base64::{engine::general_purpose, Engine as _};
use chrono::{DateTime, Utc};
use heck::{ToKebabCase, ToLowerCamelCase, ToSnakeCase, ToUpperCamelCase};
use humansize::{format_size, BINARY};
use md5;
use minijinja::value::{Rest, Value};
use num_format::{Locale, ToFormattedString};
use rand::seq::SliceRandom;
use rand::Rng;
use regex::Regex;
use serde_json::Value as JsonValue;
use sha2::{Digest, Sha256};
use std::collections::{HashMap, HashSet};
use std::path::Path;
use uuid::Uuid;

fn to_json(value: &Value) -> JsonValue {
    serde_json::to_value(value.clone()).unwrap_or(JsonValue::Null)
}

fn as_str(v: &JsonValue) -> &str {
    v.as_str().unwrap_or("")
}

fn is_null_or_empty(v: &JsonValue) -> bool {
    match v {
        JsonValue::Null => true,
        JsonValue::Bool(b) => !*b,
        JsonValue::Number(n) => n.as_i64().map(|x| x == 0).unwrap_or(false),
        JsonValue::String(s) => s.is_empty(),
        JsonValue::Array(a) => a.is_empty(),
        JsonValue::Object(o) => o.is_empty(),
    }
}

fn extract_key(value: &JsonValue, key: &str) -> JsonValue {
    match value {
        JsonValue::Object(map) => map.get(key).cloned().unwrap_or(JsonValue::Null),
        _ => JsonValue::Null,
    }
}

fn pytemplify_namespace() -> Uuid {
    Uuid::new_v5(&Uuid::NAMESPACE_DNS, b"com.github.pytemplify")
}

fn json_type_name(value: &JsonValue) -> &'static str {
    match value {
        JsonValue::Null => "null",
        JsonValue::Bool(_) => "bool",
        JsonValue::Number(_) => "number",
        JsonValue::String(_) => "string",
        JsonValue::Array(_) => "list",
        JsonValue::Object(_) => "dict",
    }
}

pub fn filter_camelcase(value: Value, _args: Rest<Value>) -> Value {
    let j = to_json(&value);
    Value::from_safe_string(as_str(&j).to_lower_camel_case())
}

pub fn filter_pascalcase(value: Value, _args: Rest<Value>) -> Value {
    let j = to_json(&value);
    Value::from_safe_string(as_str(&j).to_upper_camel_case())
}

pub fn filter_snakecase(value: Value, _args: Rest<Value>) -> Value {
    let j = to_json(&value);
    Value::from_safe_string(as_str(&j).to_snake_case())
}

pub fn filter_kebabcase(value: Value, _args: Rest<Value>) -> Value {
    let j = to_json(&value);
    Value::from_safe_string(as_str(&j).to_kebab_case())
}

pub fn filter_screamingsnakecase(value: Value, _args: Rest<Value>) -> Value {
    let j = to_json(&value);
    Value::from_safe_string(as_str(&j).to_snake_case().to_uppercase())
}

pub fn filter_slugify(value: Value, _args: Rest<Value>) -> Value {
    let j = to_json(&value);
    let s = as_str(&j).to_lowercase();
    let re = Regex::new(r"[^a-z0-9]+").unwrap();
    let slug = re.replace_all(&s, "-").trim_matches('-').to_string();
    Value::from_safe_string(slug)
}

pub fn filter_indent_custom(value: Value, args: Rest<Value>) -> Value {
    let j = to_json(&value);
    let text = as_str(&j);
    let width = args
        .get(0)
        .map(to_json)
        .and_then(|v| v.as_i64())
        .unwrap_or(4)
        .max(0) as usize;
    let first = args
        .get(1)
        .map(to_json)
        .and_then(|v| v.as_bool())
        .unwrap_or(false);
    let blank = args
        .get(2)
        .map(to_json)
        .and_then(|v| v.as_bool())
        .unwrap_or(false);

    let indent_str = " ".repeat(width);
    let mut out = String::new();
    for (index, line) in text.split_inclusive('\n').enumerate() {
        let is_first = index == 0;
        let is_blank = line.trim().is_empty();
        if (is_first && !first) || (is_blank && !blank) {
            out.push_str(line);
        } else {
            out.push_str(&indent_str);
            out.push_str(line);
        }
    }
    if !text.ends_with('\n') && !text.is_empty() {
        if text.lines().count() == 1 {
            out.clear();
            if first || (!blank && !text.trim().is_empty()) {
                out.push_str(&indent_str);
            }
            out.push_str(text);
        }
    }
    Value::from_safe_string(out)
}

pub fn filter_remove_prefix(value: Value, args: Rest<Value>) -> Value {
    let j = to_json(&value);
    let prefix = args.get(0).map(to_json).unwrap_or(JsonValue::Null);
    if let Some(prefix_str) = prefix.as_str() {
        let s = as_str(&j);
        if s.starts_with(prefix_str) {
            return Value::from_safe_string(s[prefix_str.len()..].to_string());
        }
        return Value::from_safe_string(s.to_string());
    }
    Value::from_safe_string(as_str(&j).to_string())
}

pub fn filter_remove_suffix(value: Value, args: Rest<Value>) -> Value {
    let j = to_json(&value);
    let suffix = args.get(0).map(to_json).unwrap_or(JsonValue::Null);
    if let Some(suffix_str) = suffix.as_str() {
        let s = as_str(&j);
        if s.ends_with(suffix_str) {
            return Value::from_safe_string(s[..s.len() - suffix_str.len()].to_string());
        }
        return Value::from_safe_string(s.to_string());
    }
    Value::from_safe_string(as_str(&j).to_string())
}

pub fn filter_wrap_text(value: Value, args: Rest<Value>) -> Value {
    let j = to_json(&value);
    let width = args
        .get(0)
        .map(to_json)
        .and_then(|v| v.as_i64())
        .unwrap_or(80)
        .max(1) as usize;
    let break_long_words = args
        .get(1)
        .map(to_json)
        .and_then(|v| v.as_bool())
        .unwrap_or(true);

    let words: Vec<&str> = as_str(&j).split_whitespace().collect();
    if words.is_empty() {
        return Value::from_safe_string(String::new());
    }
    let mut lines = Vec::new();
    let mut current = String::new();

    for word in words {
        if current.is_empty() {
            current.push_str(word);
            continue;
        }
        if current.len() + 1 + word.len() <= width {
            current.push(' ');
            current.push_str(word);
        } else if word.len() > width && break_long_words {
            lines.push(current);
            current = String::new();
            let mut start = 0;
            let chars: Vec<char> = word.chars().collect();
            while start < chars.len() {
                let end = (start + width).min(chars.len());
                let chunk: String = chars[start..end].iter().collect();
                if end == chars.len() {
                    current = chunk;
                } else {
                    lines.push(chunk);
                }
                start = end;
            }
        } else {
            lines.push(current);
            current = word.to_string();
        }
    }
    if !current.is_empty() {
        lines.push(current);
    }

    Value::from_safe_string(lines.join("\n"))
}

pub fn filter_truncate_custom(value: Value, args: Rest<Value>) -> Value {
    let j = to_json(&value);
    let len = args
        .get(0)
        .map(to_json)
        .and_then(|v| v.as_i64())
        .unwrap_or(0)
        .max(0) as usize;
    let end = args
        .get(1)
        .map(to_json)
        .and_then(|v| v.as_str().map(|s| s.to_string()))
        .unwrap_or_else(|| "...".to_string());

    let s = as_str(&j);
    if s.len() <= len {
        return Value::from_safe_string(s.to_string());
    }
    if len <= end.len() {
        return Value::from_safe_string(end);
    }
    Value::from_safe_string(format!("{}{}", &s[..len - end.len()], end))
}

pub fn filter_regex_search(value: Value, args: Rest<Value>) -> Value {
    let j = to_json(&value);
    let pattern = args.get(0).map(to_json).unwrap_or(JsonValue::Null);
    if let Some(pattern_str) = pattern.as_str() {
        if let Ok(re) = Regex::new(pattern_str) {
            return Value::from_serialize(re.is_match(as_str(&j)));
        }
    }
    Value::from_serialize(false)
}

pub fn filter_regex_findall(value: Value, args: Rest<Value>) -> Value {
    let j = to_json(&value);
    let pattern = args.get(0).map(to_json).unwrap_or(JsonValue::Null);
    if let Some(pattern_str) = pattern.as_str() {
        if let Ok(re) = Regex::new(pattern_str) {
            let matches: Vec<JsonValue> = re
                .find_iter(as_str(&j))
                .map(|m| JsonValue::String(m.as_str().to_string()))
                .collect();
            return Value::from_serialize(matches);
        }
    }
    Value::from_serialize(Vec::<JsonValue>::new())
}

pub fn filter_quote_string(value: Value, args: Rest<Value>) -> Value {
    let j = to_json(&value);
    let quote = args
        .get(0)
        .map(to_json)
        .and_then(|v| v.as_str().map(|s| s.to_string()))
        .unwrap_or_else(|| "\"".to_string());
    let escaped = as_str(&j).replace(&quote, &format!("\\{}", quote));
    Value::from_safe_string(format!("{}{}{}", quote, escaped, quote))
}

pub fn filter_normalize(value: Value, _args: Rest<Value>) -> Value {
    let j = to_json(&value);
    let s = as_str(&j).to_lowercase();
    let re = Regex::new(r"[^a-z0-9]+").unwrap();
    let normalized = re.replace_all(&s, "_").trim_matches('_').to_string();
    Value::from_safe_string(normalized)
}

pub fn filter_uppercase(value: Value, _args: Rest<Value>) -> Value {
    let j = to_json(&value);
    Value::from_safe_string(as_str(&j).to_uppercase())
}

pub fn filter_lowercase(value: Value, _args: Rest<Value>) -> Value {
    let j = to_json(&value);
    Value::from_safe_string(as_str(&j).to_lowercase())
}

pub fn filter_titlecase(value: Value, _args: Rest<Value>) -> Value {
    let j = to_json(&value);
    let input = as_str(&j);
    let words: Vec<String> = input
        .split_whitespace()
        .map(|word| {
            let mut chars = word.chars();
            match chars.next() {
                Some(first) => format!("{}{}", first.to_uppercase(), chars.as_str().to_lowercase()),
                None => String::new(),
            }
        })
        .collect();
    Value::from_safe_string(words.join(" "))
}

pub fn filter_default(value: Value, args: Rest<Value>) -> Value {
    let fallback = args
        .get(0)
        .cloned()
        .unwrap_or_else(|| Value::from_serialize(JsonValue::Null));
    let j = to_json(&value);
    if is_null_or_empty(&j) {
        fallback
    } else {
        Value::from_serialize(j)
    }
}

pub fn filter_contains(value: Value, args: Rest<Value>) -> Value {
    let needle = args.get(0).map(to_json).unwrap_or(JsonValue::Null);
    let j = to_json(&value);

    match j {
        JsonValue::Array(arr) => Value::from_serialize(arr.iter().any(|v| v == &needle)),
        JsonValue::String(s) => {
            Value::from_serialize(needle.as_str().map(|n| s.contains(n)).unwrap_or(false))
        }
        _ => Value::from_serialize(false),
    }
}

pub fn filter_trim(value: Value, _args: Rest<Value>) -> Value {
    let j = to_json(&value);
    Value::from_safe_string(as_str(&j).trim().to_string())
}

pub fn filter_trim_start(value: Value, _args: Rest<Value>) -> Value {
    let j = to_json(&value);
    Value::from_safe_string(as_str(&j).trim_start().to_string())
}

pub fn filter_trim_end(value: Value, _args: Rest<Value>) -> Value {
    let j = to_json(&value);
    Value::from_safe_string(as_str(&j).trim_end().to_string())
}

pub fn filter_startswith(value: Value, args: Rest<Value>) -> Value {
    let needle = args.get(0).map(to_json).unwrap_or(JsonValue::Null);
    let j = to_json(&value);
    if let Some(s) = j.as_str() {
        return Value::from_serialize(needle.as_str().map(|n| s.starts_with(n)).unwrap_or(false));
    }
    Value::from_serialize(false)
}

pub fn filter_endswith(value: Value, args: Rest<Value>) -> Value {
    let needle = args.get(0).map(to_json).unwrap_or(JsonValue::Null);
    let j = to_json(&value);
    if let Some(s) = j.as_str() {
        return Value::from_serialize(needle.as_str().map(|n| s.ends_with(n)).unwrap_or(false));
    }
    Value::from_serialize(false)
}

pub fn filter_replace(value: Value, args: Rest<Value>) -> Value {
    let j = to_json(&value);
    let s = as_str(&j);
    let from = args.get(0).map(to_json).unwrap_or(JsonValue::Null);
    let to = args.get(1).map(to_json).unwrap_or(JsonValue::Null);
    if let Some(pattern) = from.as_str() {
        let replacement = to.as_str().unwrap_or("");
        return Value::from_safe_string(s.replace(pattern, replacement));
    }
    Value::from_safe_string(s.to_string())
}

pub fn filter_split(value: Value, args: Rest<Value>) -> Value {
    let j = to_json(&value);
    let s = as_str(&j);
    let delimiter = args.get(0).map(to_json).unwrap_or(JsonValue::Null);
    let delim = delimiter.as_str().unwrap_or("");
    let parts: Vec<JsonValue> = if delim.is_empty() {
        s.chars()
            .map(|c| JsonValue::String(c.to_string()))
            .collect()
    } else {
        s.split(delim)
            .map(|part| JsonValue::String(part.to_string()))
            .collect()
    };
    Value::from_serialize(parts)
}

pub fn filter_join(value: Value, args: Rest<Value>) -> Value {
    let j = to_json(&value);
    let delimiter = args.get(0).map(to_json).unwrap_or(JsonValue::Null);
    let delim = delimiter.as_str().unwrap_or("");
    if let Some(arr) = j.as_array() {
        let joined = arr
            .iter()
            .map(|item| match item {
                JsonValue::String(s) => s.clone(),
                _ => item.to_string(),
            })
            .collect::<Vec<String>>()
            .join(delim);
        return Value::from_safe_string(joined);
    }
    Value::from_safe_string(String::new())
}

pub fn filter_pad_start(value: Value, args: Rest<Value>) -> Value {
    let j = to_json(&value);
    let s = as_str(&j);
    let width = args
        .get(0)
        .map(to_json)
        .and_then(|v| v.as_i64())
        .unwrap_or(s.len() as i64);
    let fill = args.get(1).map(to_json).unwrap_or(JsonValue::Null);
    let fill_char = fill.as_str().and_then(|s| s.chars().next()).unwrap_or(' ');
    if (s.len() as i64) >= width {
        return Value::from_safe_string(s.to_string());
    }
    let padding = fill_char
        .to_string()
        .repeat((width as usize).saturating_sub(s.len()));
    Value::from_safe_string(format!("{}{}", padding, s))
}

pub fn filter_pad_end(value: Value, args: Rest<Value>) -> Value {
    let j = to_json(&value);
    let s = as_str(&j);
    let width = args
        .get(0)
        .map(to_json)
        .and_then(|v| v.as_i64())
        .unwrap_or(s.len() as i64);
    let fill = args.get(1).map(to_json).unwrap_or(JsonValue::Null);
    let fill_char = fill.as_str().and_then(|s| s.chars().next()).unwrap_or(' ');
    if (s.len() as i64) >= width {
        return Value::from_safe_string(s.to_string());
    }
    let padding = fill_char
        .to_string()
        .repeat((width as usize).saturating_sub(s.len()));
    Value::from_safe_string(format!("{}{}", s, padding))
}

pub fn filter_capitalize(value: Value, _args: Rest<Value>) -> Value {
    let j = to_json(&value);
    let s = as_str(&j);
    let mut chars = s.chars();
    if let Some(first) = chars.next() {
        return Value::from_safe_string(format!(
            "{}{}",
            first.to_uppercase(),
            chars.as_str().to_lowercase()
        ));
    }
    Value::from_safe_string(String::new())
}

pub fn filter_remove(value: Value, args: Rest<Value>) -> Value {
    let j = to_json(&value);
    let s = as_str(&j);
    let needle = args.get(0).map(to_json).unwrap_or(JsonValue::Null);
    if let Some(pattern) = needle.as_str() {
        return Value::from_safe_string(s.replace(pattern, ""));
    }
    Value::from_safe_string(s.to_string())
}

pub fn filter_repeat(value: Value, args: Rest<Value>) -> Value {
    let j = to_json(&value);
    let s = as_str(&j);
    let count = args
        .get(0)
        .map(to_json)
        .and_then(|v| v.as_i64())
        .unwrap_or(1)
        .max(0) as usize;
    Value::from_safe_string(s.repeat(count))
}

pub fn filter_reverse(value: Value, _args: Rest<Value>) -> Value {
    let j = to_json(&value);
    match j {
        JsonValue::Array(arr) => {
            let mut out = arr.clone();
            out.reverse();
            Value::from_serialize(out)
        }
        JsonValue::String(s) => Value::from_safe_string(s.chars().rev().collect()),
        _ => Value::from_serialize(JsonValue::Null),
    }
}

pub fn filter_truncate(value: Value, args: Rest<Value>) -> Value {
    let j = to_json(&value);
    let s = as_str(&j);
    let limit = args
        .get(0)
        .map(to_json)
        .and_then(|v| v.as_i64())
        .unwrap_or(s.len() as i64)
        .max(0) as usize;
    let suffix = args
        .get(1)
        .map(to_json)
        .and_then(|v| v.as_str().map(|s| s.to_string()))
        .unwrap_or_default();

    let mut chars: Vec<char> = s.chars().collect();
    if chars.len() <= limit {
        return Value::from_safe_string(s.to_string());
    }
    chars.truncate(limit);
    let mut out: String = chars.into_iter().collect();
    out.push_str(&suffix);
    Value::from_safe_string(out)
}

pub fn filter_slice(value: Value, args: Rest<Value>) -> Value {
    let j = to_json(&value);
    let start = args
        .get(0)
        .map(to_json)
        .and_then(|v| v.as_i64())
        .unwrap_or(0);
    let end = args.get(1).map(to_json).and_then(|v| v.as_i64());

    match j {
        JsonValue::Array(arr) => {
            let len = arr.len() as i64;
            let start_idx = start.max(0).min(len) as usize;
            let end_idx = end.unwrap_or(len).max(0).min(len) as usize;
            if start_idx >= end_idx {
                return Value::from_serialize(Vec::<JsonValue>::new());
            }
            Value::from_serialize(arr[start_idx..end_idx].to_vec())
        }
        JsonValue::String(s) => {
            let chars: Vec<char> = s.chars().collect();
            let len = chars.len() as i64;
            let start_idx = start.max(0).min(len) as usize;
            let end_idx = end.unwrap_or(len).max(0).min(len) as usize;
            if start_idx >= end_idx {
                return Value::from_safe_string(String::new());
            }
            let out: String = chars[start_idx..end_idx].iter().collect();
            Value::from_safe_string(out)
        }
        _ => Value::from_serialize(JsonValue::Null),
    }
}

pub fn filter_length(value: Value, _args: Rest<Value>) -> Value {
    let j = to_json(&value);
    let len = match j {
        JsonValue::Array(arr) => arr.len() as i64,
        JsonValue::String(s) => s.chars().count() as i64,
        JsonValue::Object(map) => map.len() as i64,
        _ => 0,
    };
    Value::from_serialize(len)
}

pub fn filter_first(value: Value, _args: Rest<Value>) -> Value {
    let j = to_json(&value);
    match j {
        JsonValue::Array(arr) => {
            Value::from_serialize(arr.first().cloned().unwrap_or(JsonValue::Null))
        }
        JsonValue::String(s) => {
            Value::from_safe_string(s.chars().next().map(|c| c.to_string()).unwrap_or_default())
        }
        _ => Value::from_serialize(JsonValue::Null),
    }
}

pub fn filter_last(value: Value, _args: Rest<Value>) -> Value {
    let j = to_json(&value);
    match j {
        JsonValue::Array(arr) => {
            Value::from_serialize(arr.last().cloned().unwrap_or(JsonValue::Null))
        }
        JsonValue::String(s) => {
            Value::from_safe_string(s.chars().last().map(|c| c.to_string()).unwrap_or_default())
        }
        _ => Value::from_serialize(JsonValue::Null),
    }
}

pub fn filter_sum(value: Value, _args: Rest<Value>) -> Value {
    let j = to_json(&value);
    if let Some(arr) = j.as_array() {
        let mut int_total: i64 = 0;
        let mut float_total: f64 = 0.0;
        let mut all_int = true;

        for v in arr {
            if v.is_i64() {
                if let Some(n) = v.as_i64() {
                    int_total = int_total.saturating_add(n);
                    float_total += n as f64;
                }
            } else if v.is_u64() {
                if let Some(n) = v.as_u64() {
                    if n <= i64::MAX as u64 {
                        int_total = int_total.saturating_add(n as i64);
                        float_total += n as f64;
                    } else {
                        all_int = false;
                        float_total += n as f64;
                    }
                }
            } else if let Some(n) = v.as_f64() {
                all_int = false;
                float_total += n;
            }
        }

        if all_int {
            return Value::from_serialize(int_total);
        }
        return Value::from_serialize(float_total);
    }
    Value::from_serialize(0)
}

pub fn filter_min(value: Value, _args: Rest<Value>) -> Value {
    let j = to_json(&value);
    if let Some(arr) = j.as_array() {
        let mut int_min: Option<i64> = None;
        let mut float_min: Option<f64> = None;
        let mut all_int = true;

        for v in arr {
            if v.is_i64() {
                if let Some(n) = v.as_i64() {
                    int_min = Some(int_min.map(|m| m.min(n)).unwrap_or(n));
                    float_min = Some(float_min.map(|m| m.min(n as f64)).unwrap_or(n as f64));
                }
            } else if v.is_u64() {
                if let Some(n) = v.as_u64() {
                    if n <= i64::MAX as u64 {
                        let n_i = n as i64;
                        int_min = Some(int_min.map(|m| m.min(n_i)).unwrap_or(n_i));
                        float_min = Some(float_min.map(|m| m.min(n as f64)).unwrap_or(n as f64));
                    } else {
                        all_int = false;
                        float_min = Some(float_min.map(|m| m.min(n as f64)).unwrap_or(n as f64));
                    }
                }
            } else if let Some(n) = v.as_f64() {
                all_int = false;
                float_min = Some(float_min.map(|m| m.min(n)).unwrap_or(n));
            }
        }

        if all_int {
            return int_min
                .map(Value::from_serialize)
                .unwrap_or_else(|| Value::from_serialize(JsonValue::Null));
        }
        return float_min
            .map(Value::from_serialize)
            .unwrap_or_else(|| Value::from_serialize(JsonValue::Null));
    }
    Value::from_serialize(JsonValue::Null)
}

pub fn filter_max(value: Value, _args: Rest<Value>) -> Value {
    let j = to_json(&value);
    if let Some(arr) = j.as_array() {
        let mut int_max: Option<i64> = None;
        let mut float_max: Option<f64> = None;
        let mut all_int = true;

        for v in arr {
            if v.is_i64() {
                if let Some(n) = v.as_i64() {
                    int_max = Some(int_max.map(|m| m.max(n)).unwrap_or(n));
                    float_max = Some(float_max.map(|m| m.max(n as f64)).unwrap_or(n as f64));
                }
            } else if v.is_u64() {
                if let Some(n) = v.as_u64() {
                    if n <= i64::MAX as u64 {
                        let n_i = n as i64;
                        int_max = Some(int_max.map(|m| m.max(n_i)).unwrap_or(n_i));
                        float_max = Some(float_max.map(|m| m.max(n as f64)).unwrap_or(n as f64));
                    } else {
                        all_int = false;
                        float_max = Some(float_max.map(|m| m.max(n as f64)).unwrap_or(n as f64));
                    }
                }
            } else if let Some(n) = v.as_f64() {
                all_int = false;
                float_max = Some(float_max.map(|m| m.max(n)).unwrap_or(n));
            }
        }

        if all_int {
            return int_max
                .map(Value::from_serialize)
                .unwrap_or_else(|| Value::from_serialize(JsonValue::Null));
        }
        return float_max
            .map(Value::from_serialize)
            .unwrap_or_else(|| Value::from_serialize(JsonValue::Null));
    }
    Value::from_serialize(JsonValue::Null)
}

pub fn filter_round(value: Value, args: Rest<Value>) -> Value {
    let j = to_json(&value);
    let decimals = args
        .get(0)
        .map(to_json)
        .and_then(|v| v.as_i64())
        .unwrap_or(0);
    if let Some(num) = j.as_f64().or_else(|| j.as_i64().map(|n| n as f64)) {
        let factor = 10_f64.powi(decimals as i32);
        let rounded = (num * factor).round() / factor;
        return Value::from_serialize(rounded);
    }
    Value::from_serialize(0)
}

pub fn filter_avg(value: Value, _args: Rest<Value>) -> Value {
    let j = to_json(&value);
    if let Some(arr) = j.as_array() {
        let mut total = 0.0;
        let mut count = 0.0;
        let mut all_int = true;

        for v in arr {
            if v.is_i64() {
                if let Some(n) = v.as_i64() {
                    total += n as f64;
                    count += 1.0;
                }
            } else if v.is_u64() {
                if let Some(n) = v.as_u64() {
                    total += n as f64;
                    count += 1.0;
                }
            } else if let Some(n) = v.as_f64() {
                all_int = false;
                total += n;
                count += 1.0;
            }
        }
        if count == 0.0 {
            return Value::from_serialize(0);
        }
        let avg = total / count;
        if all_int && avg.fract() == 0.0 {
            return Value::from_serialize(avg as i64);
        }
        return Value::from_serialize(avg);
    }
    Value::from_serialize(0)
}

pub fn filter_median(value: Value, _args: Rest<Value>) -> Value {
    let j = to_json(&value);
    if let Some(arr) = j.as_array() {
        let mut nums: Vec<f64> = arr
            .iter()
            .filter_map(|v| v.as_f64().or_else(|| v.as_i64().map(|n| n as f64)))
            .collect();
        if nums.is_empty() {
            return Value::from_serialize(0);
        }
        nums.sort_by(|a, b| a.partial_cmp(b).unwrap_or(std::cmp::Ordering::Equal));
        let mid = nums.len() / 2;
        if nums.len() % 2 == 0 {
            return Value::from_serialize((nums[mid - 1] + nums[mid]) / 2.0);
        }
        return Value::from_serialize(nums[mid]);
    }
    Value::from_serialize(0)
}

pub fn filter_unique_by(value: Value, args: Rest<Value>) -> Value {
    let j = to_json(&value);
    let key = args
        .get(0)
        .map(to_json)
        .and_then(|v| v.as_str().map(|s| s.to_string()));
    if let Some(arr) = j.as_array() {
        let mut seen = HashSet::new();
        let mut out = Vec::new();
        for item in arr {
            let signature = if let Some(ref key) = key {
                extract_key(item, key).to_string()
            } else {
                item.to_string()
            };
            if seen.insert(signature) {
                out.push(item.clone());
            }
        }
        return Value::from_serialize(out);
    }
    Value::from_serialize(JsonValue::Null)
}

pub fn filter_dict_merge(value: Value, args: Rest<Value>) -> Value {
    let mut base = to_json(&value);
    if let JsonValue::Object(ref mut map) = base {
        for arg in args.iter() {
            if let JsonValue::Object(extra) = to_json(arg) {
                for (k, v) in extra {
                    map.insert(k, v);
                }
            }
        }
        return Value::from_serialize(base);
    }
    Value::from_serialize(base)
}

pub fn filter_uuid_generate(value: Value, args: Rest<Value>) -> Value {
    let j = to_json(&value);
    let value_str = j.as_str().map(|s| s.to_string());
    if value_str.is_none() || value_str.as_deref().unwrap_or("").is_empty() {
        return Value::from_safe_string(Uuid::new_v4().to_string());
    }

    let namespace_arg = args
        .get(0)
        .map(to_json)
        .and_then(|v| v.as_str().map(|s| s.to_string()));

    let namespace = if let Some(ns) = namespace_arg {
        match ns.as_str() {
            "dns" => Uuid::NAMESPACE_DNS,
            "url" => Uuid::NAMESPACE_URL,
            "oid" => Uuid::NAMESPACE_OID,
            "x500" => Uuid::NAMESPACE_X500,
            "pytemplify" => pytemplify_namespace(),
            _ => Uuid::parse_str(&ns)
                .unwrap_or_else(|_| Uuid::new_v5(&pytemplify_namespace(), ns.as_bytes())),
        }
    } else {
        pytemplify_namespace()
    };

    let name = value_str.unwrap_or_default();
    Value::from_safe_string(Uuid::new_v5(&namespace, name.as_bytes()).to_string())
}

pub fn filter_regex_replace(value: Value, args: Rest<Value>) -> Value {
    let j = to_json(&value);
    let s = as_str(&j);
    let pattern_json = args.get(0).map(|v| to_json(v)).unwrap_or(JsonValue::Null);
    let repl_json = args.get(1).map(|v| to_json(v)).unwrap_or(JsonValue::Null);
    let count = args
        .get(2)
        .map(|v| to_json(v))
        .and_then(|v| v.as_i64())
        .unwrap_or(0)
        .max(0) as usize;
    let pattern = pattern_json.as_str().unwrap_or("");
    let repl = repl_json.as_str().unwrap_or("");
    let re = Regex::new(pattern).unwrap_or_else(|_| Regex::new("").unwrap());
    if count == 0 {
        return Value::from_safe_string(re.replace_all(s, repl).to_string());
    }
    let mut result = s.to_string();
    let mut remaining = count;
    while remaining > 0 {
        let replaced = re.replace(&result, repl).to_string();
        if replaced == result {
            break;
        }
        result = replaced;
        remaining -= 1;
    }
    Value::from_safe_string(result)
}

pub fn filter_ternary(value: Value, args: Rest<Value>) -> Value {
    let j = to_json(&value);
    let true_val = args
        .get(0)
        .cloned()
        .unwrap_or_else(|| Value::from_serialize(JsonValue::Null));
    let false_val = args
        .get(1)
        .cloned()
        .unwrap_or_else(|| Value::from_serialize(JsonValue::Null));
    let truthy = match &j {
        JsonValue::Bool(b) => *b,
        JsonValue::Null => false,
        JsonValue::Number(n) => n.as_i64().map(|x| x != 0).unwrap_or(true),
        JsonValue::String(s) => !s.is_empty(),
        JsonValue::Array(a) => !a.is_empty(),
        JsonValue::Object(o) => !o.is_empty(),
    };
    if truthy {
        true_val
    } else {
        false_val
    }
}

pub fn filter_coalesce(value: Value, args: Rest<Value>) -> Value {
    let j = to_json(&value);
    if !is_null_or_empty(&j) {
        return Value::from_serialize(j);
    }
    for v in args.iter() {
        let jv = to_json(v);
        if !is_null_or_empty(&jv) {
            return Value::from_serialize(jv);
        }
    }
    Value::from_serialize(JsonValue::Null)
}

pub fn filter_default_if_none(value: Value, args: Rest<Value>) -> Value {
    let j = to_json(&value);
    if j.is_null() {
        return args
            .get(0)
            .cloned()
            .unwrap_or_else(|| Value::from_serialize(JsonValue::Null));
    }
    Value::from_serialize(j)
}

pub fn filter_type_name(value: Value, _args: Rest<Value>) -> Value {
    let j = to_json(&value);
    Value::from_safe_string(json_type_name(&j).to_string())
}

pub fn filter_is_list(value: Value, _args: Rest<Value>) -> Value {
    let j = to_json(&value);
    Value::from_serialize(matches!(j, JsonValue::Array(_)))
}

pub fn filter_is_dict(value: Value, _args: Rest<Value>) -> Value {
    let j = to_json(&value);
    Value::from_serialize(matches!(j, JsonValue::Object(_)))
}

pub fn filter_is_string(value: Value, _args: Rest<Value>) -> Value {
    let j = to_json(&value);
    Value::from_serialize(matches!(j, JsonValue::String(_)))
}

pub fn filter_is_number(value: Value, _args: Rest<Value>) -> Value {
    let j = to_json(&value);
    Value::from_serialize(matches!(j, JsonValue::Number(_)))
}

pub fn filter_is_even(value: Value, _args: Rest<Value>) -> Value {
    let j = to_json(&value);
    if let Some(n) = j.as_i64() {
        return Value::from_serialize(n % 2 == 0);
    }
    Value::from_serialize(false)
}

pub fn filter_is_odd(value: Value, _args: Rest<Value>) -> Value {
    let j = to_json(&value);
    if let Some(n) = j.as_i64() {
        return Value::from_serialize(n % 2 != 0);
    }
    Value::from_serialize(false)
}

pub fn filter_hash_md5(value: Value, _args: Rest<Value>) -> Value {
    let j = to_json(&value);
    let s = as_str(&j);
    let digest = md5::compute(s.as_bytes());
    Value::from_safe_string(format!("{:x}", digest))
}

pub fn filter_hash_sha256(value: Value, _args: Rest<Value>) -> Value {
    let j = to_json(&value);
    let s = as_str(&j);
    let mut hasher = Sha256::new();
    hasher.update(s.as_bytes());
    let result = hasher.finalize();
    Value::from_safe_string(format!("{:x}", result))
}

pub fn filter_b64encode(value: Value, _args: Rest<Value>) -> Value {
    let j = to_json(&value);
    let encoded = general_purpose::STANDARD.encode(as_str(&j));
    Value::from_safe_string(encoded)
}

pub fn filter_b64decode(value: Value, _args: Rest<Value>) -> Value {
    let j = to_json(&value);
    match general_purpose::STANDARD.decode(as_str(&j)) {
        Ok(bytes) => Value::from_safe_string(String::from_utf8_lossy(&bytes).to_string()),
        Err(_) => Value::from_safe_string(as_str(&j).to_string()),
    }
}

pub fn filter_random_string(value: Value, args: Rest<Value>) -> Value {
    let j = to_json(&value);
    let len = j
        .as_i64()
        .or_else(|| args.get(0).map(to_json).and_then(|v| v.as_i64()))
        .unwrap_or(10)
        .max(0) as usize;
    let charset = args
        .get(0)
        .map(to_json)
        .and_then(|v| v.as_str().map(|s| s.to_string()))
        .or_else(|| {
            args.get(1)
                .map(to_json)
                .and_then(|v| v.as_str().map(|s| s.to_string()))
        })
        .unwrap_or_else(|| "alphanumeric".to_string());

    let chars = match charset.as_str() {
        "alpha" => "abcdefghijklmnopqrstuvwxyzABCDEFGHIJKLMNOPQRSTUVWXYZ".to_string(),
        "numeric" => "0123456789".to_string(),
        "hex" => "0123456789abcdef".to_string(),
        _ => "abcdefghijklmnopqrstuvwxyzABCDEFGHIJKLMNOPQRSTUVWXYZ0123456789".to_string(),
    };

    let pool: Vec<char> = chars.chars().collect();
    let mut rng = rand::thread_rng();
    let result: String = (0..len)
        .map(|_| pool.choose(&mut rng).copied().unwrap_or('0'))
        .collect();

    Value::from_safe_string(result)
}

pub fn filter_random_int(value: Value, args: Rest<Value>) -> Value {
    let j = to_json(&value);
    let min = j
        .as_i64()
        .or_else(|| args.get(0).map(to_json).and_then(|v| v.as_i64()))
        .unwrap_or(0);
    let max = args
        .get(0)
        .map(to_json)
        .and_then(|v| v.as_i64())
        .or_else(|| args.get(1).map(to_json).and_then(|v| v.as_i64()))
        .unwrap_or(100);
    let (low, high) = if min <= max { (min, max) } else { (max, min) };
    let mut rng = rand::thread_rng();
    let value = rng.gen_range(low..=high);
    Value::from_serialize(value)
}

pub fn filter_abs_value(value: Value, _args: Rest<Value>) -> Value {
    let j = to_json(&value);
    if let Some(n) = j.as_i64() {
        return Value::from_serialize(n.abs());
    }
    if let Some(n) = j.as_u64() {
        return Value::from_serialize(n);
    }
    if let Some(n) = j.as_f64() {
        return Value::from_serialize(n.abs());
    }
    Value::from_serialize(0)
}

pub fn filter_clamp(value: Value, args: Rest<Value>) -> Value {
    let j = to_json(&value);
    let min_raw = args.get(0).map(to_json).unwrap_or(JsonValue::Null);
    let max_raw = args.get(1).map(to_json).unwrap_or(min_raw.clone());

    let min_val = min_raw
        .as_f64()
        .or_else(|| min_raw.as_i64().map(|n| n as f64))
        .unwrap_or(0.0);
    let max_val = max_raw
        .as_f64()
        .or_else(|| max_raw.as_i64().map(|n| n as f64))
        .unwrap_or(min_val);

    if let Some(n) = j.as_i64() {
        if min_raw.is_i64() && max_raw.is_i64() {
            let min_i = min_raw.as_i64().unwrap_or(min_val as i64);
            let max_i = max_raw.as_i64().unwrap_or(max_val as i64);
            return Value::from_serialize(n.max(min_i).min(max_i));
        }
        return Value::from_serialize((n as f64).max(min_val).min(max_val));
    }
    if let Some(n) = j.as_f64() {
        return Value::from_serialize(n.max(min_val).min(max_val));
    }
    Value::from_serialize(min_val)
}

pub fn filter_bool_to_string(value: Value, args: Rest<Value>) -> Value {
    let j = to_json(&value);
    let true_str = args
        .get(0)
        .map(to_json)
        .and_then(|v| v.as_str().map(|s| s.to_string()))
        .unwrap_or_else(|| "true".to_string());
    let false_str = args
        .get(1)
        .map(to_json)
        .and_then(|v| v.as_str().map(|s| s.to_string()))
        .unwrap_or_else(|| "false".to_string());
    if j.as_bool().unwrap_or(false) {
        Value::from_safe_string(true_str)
    } else {
        Value::from_safe_string(false_str)
    }
}

pub fn filter_file_extension(value: Value, _args: Rest<Value>) -> Value {
    let j = to_json(&value);
    let path = Path::new(as_str(&j));
    let ext = path.extension().and_then(|s| s.to_str()).unwrap_or("");
    Value::from_safe_string(ext.to_string())
}

pub fn filter_file_basename(value: Value, _args: Rest<Value>) -> Value {
    let j = to_json(&value);
    let path = Path::new(as_str(&j));
    let base = path.file_name().and_then(|s| s.to_str()).unwrap_or("");
    Value::from_safe_string(base.to_string())
}

pub fn filter_file_dirname(value: Value, _args: Rest<Value>) -> Value {
    let j = to_json(&value);
    let path = Path::new(as_str(&j));
    let dir = path.parent().and_then(|p| p.to_str()).unwrap_or("");
    Value::from_safe_string(dir.to_string())
}

pub fn filter_safe_divide(value: Value, args: Rest<Value>) -> Value {
    let j = to_json(&value);
    let divisor = args
        .get(0)
        .map(to_json)
        .and_then(|v| v.as_f64().or_else(|| v.as_i64().map(|n| n as f64)))
        .unwrap_or(1.0);
    let default = args
        .get(1)
        .map(to_json)
        .and_then(|v| v.as_f64().or_else(|| v.as_i64().map(|n| n as f64)))
        .unwrap_or(0.0);
    let dividend = j
        .as_f64()
        .or_else(|| j.as_i64().map(|n| n as f64))
        .unwrap_or(0.0);
    if divisor == 0.0 {
        return Value::from_serialize(default);
    }
    Value::from_serialize(dividend / divisor)
}

pub fn filter_map_value(value: Value, args: Rest<Value>) -> Value {
    let j = to_json(&value);
    let mapping = args.get(0).map(to_json).unwrap_or(JsonValue::Null);
    let default = args.get(1).map(to_json).unwrap_or(JsonValue::Null);
    if let JsonValue::Object(map) = mapping {
        let key = match &j {
            JsonValue::String(s) => s.clone(),
            JsonValue::Number(_) | JsonValue::Bool(_) => j.to_string(),
            _ => j.to_string(),
        };
        return Value::from_serialize(map.get(&key).cloned().unwrap_or(default));
    }
    Value::from_serialize(default)
}

pub fn filter_get_attr(value: Value, args: Rest<Value>) -> Value {
    let j = to_json(&value);
    let key = args.get(0).map(to_json).unwrap_or(JsonValue::Null);
    let default = args.get(1).map(to_json).unwrap_or(JsonValue::Null);
    if let Some(key_str) = key.as_str() {
        if let JsonValue::Object(map) = j {
            return Value::from_serialize(map.get(key_str).cloned().unwrap_or(default));
        }
    }
    Value::from_serialize(default)
}

pub fn filter_get_item(value: Value, args: Rest<Value>) -> Value {
    let j = to_json(&value);
    let key = args.get(0).map(to_json).unwrap_or(JsonValue::Null);
    let default = args.get(1).map(to_json).unwrap_or(JsonValue::Null);
    match (j, key) {
        (JsonValue::Object(map), JsonValue::String(k)) => {
            Value::from_serialize(map.get(&k).cloned().unwrap_or(default))
        }
        (JsonValue::Array(arr), JsonValue::Number(n)) => {
            if let Some(idx) = n.as_i64() {
                if idx >= 0 && (idx as usize) < arr.len() {
                    return Value::from_serialize(arr[idx as usize].clone());
                }
            }
            Value::from_serialize(default)
        }
        _ => Value::from_serialize(default),
    }
}

pub fn filter_flatten(value: Value, args: Rest<Value>) -> Value {
    let j = to_json(&value);
    let levels = args
        .get(0)
        .map(to_json)
        .and_then(|v| v.as_i64())
        .unwrap_or(1);

    fn flatten_recursive(input: &JsonValue, remaining: i64, output: &mut Vec<JsonValue>) {
        if remaining == 0 {
            output.push(input.clone());
            return;
        }
        if let JsonValue::Array(arr) = input {
            if remaining == 1 {
                for item in arr {
                    if let JsonValue::Array(inner) = item {
                        output.extend(inner.iter().cloned());
                    } else {
                        output.push(item.clone());
                    }
                }
                return;
            }
            for item in arr {
                if remaining == -1 {
                    flatten_recursive(item, -1, output);
                } else {
                    flatten_recursive(item, remaining - 1, output);
                }
            }
        } else {
            output.push(input.clone());
        }
    }

    let mut out = Vec::new();
    flatten_recursive(&j, levels, &mut out);
    Value::from_serialize(out)
}

pub fn filter_unique(value: Value, _args: Rest<Value>) -> Value {
    let j = to_json(&value);
    if let Some(arr) = j.as_array() {
        let mut seen = HashSet::new();
        let mut out = Vec::new();
        for v in arr {
            let key = v.to_string();
            if seen.insert(key) {
                out.push(v.clone());
            }
        }
        Value::from_serialize(out)
    } else {
        Value::from_serialize(JsonValue::Null)
    }
}

pub fn filter_compact(value: Value, _args: Rest<Value>) -> Value {
    let j = to_json(&value);
    if let Some(arr) = j.as_array() {
        let out: Vec<JsonValue> = arr
            .iter()
            .filter(|v| !is_null_or_empty(v))
            .cloned()
            .collect();
        Value::from_serialize(out)
    } else {
        Value::from_serialize(JsonValue::Null)
    }
}

pub fn filter_pluck(value: Value, args: Rest<Value>) -> Value {
    let j = to_json(&value);
    let key_json = args.get(0).map(|v| to_json(v)).unwrap_or(JsonValue::Null);
    let key = key_json.as_str().unwrap_or("");
    if let Some(arr) = j.as_array() {
        let out: Vec<JsonValue> = arr
            .iter()
            .filter_map(|item| {
                let v = extract_key(item, key);
                if v.is_null() {
                    None
                } else {
                    Some(v)
                }
            })
            .collect();
        Value::from_serialize(out)
    } else {
        Value::from_serialize(JsonValue::Null)
    }
}

pub fn filter_where(value: Value, args: Rest<Value>) -> Value {
    let j = to_json(&value);
    let key_json = args.get(0).map(|v| to_json(v)).unwrap_or(JsonValue::Null);
    let key = key_json.as_str().unwrap_or("");
    let expected = args.get(1).map(to_json);
    if let Some(arr) = j.as_array() {
        let out: Vec<JsonValue> = arr
            .iter()
            .filter(|item| match item {
                JsonValue::Object(_) => {
                    let v = extract_key(item, key);
                    match &expected {
                        Some(val) => v == *val,
                        None => !is_null_or_empty(&v),
                    }
                }
                _ => false,
            })
            .cloned()
            .collect();
        Value::from_serialize(out)
    } else {
        Value::from_serialize(JsonValue::Null)
    }
}

pub fn filter_sort_by(value: Value, args: Rest<Value>) -> Value {
    let j = to_json(&value);
    let key_json = args.get(0).map(|v| to_json(v)).unwrap_or(JsonValue::Null);
    let key = key_json.as_str().unwrap_or("");
    if let Some(arr) = j.as_array() {
        let mut out = arr.clone();
        out.sort_by(|a, b| {
            extract_key(a, key)
                .to_string()
                .cmp(&extract_key(b, key).to_string())
        });
        Value::from_serialize(out)
    } else {
        Value::from_serialize(JsonValue::Null)
    }
}

pub fn filter_group_by(value: Value, args: Rest<Value>) -> Value {
    let j = to_json(&value);
    let key_json = args.get(0).map(|v| to_json(v)).unwrap_or(JsonValue::Null);
    let key = key_json.as_str().unwrap_or("");
    let mut map: HashMap<String, Vec<JsonValue>> = HashMap::new();
    if let Some(arr) = j.as_array() {
        for item in arr {
            if let JsonValue::Object(_) = item {
                if let Some(kv) = extract_key(item, key).as_str() {
                    map.entry(kv.to_string()).or_default().push(item.clone());
                }
            }
        }
    }
    Value::from_serialize(map)
}

pub fn filter_chunk(value: Value, args: Rest<Value>) -> Value {
    let j = to_json(&value);
    let size = args
        .get(0)
        .map(to_json)
        .and_then(|v| v.as_i64())
        .unwrap_or(0)
        .max(1) as usize;
    if let Some(arr) = j.as_array() {
        let mut chunks = Vec::new();
        let mut index = 0;
        while index < arr.len() {
            let end = (index + size).min(arr.len());
            chunks.push(JsonValue::Array(arr[index..end].to_vec()));
            index = end;
        }
        return Value::from_serialize(chunks);
    }
    Value::from_serialize(Vec::<JsonValue>::new())
}

pub fn filter_merge_dicts(value: Value, args: Rest<Value>) -> Value {
    let mut base = to_json(&value);
    if let JsonValue::Object(ref mut map) = base {
        for arg in args.iter() {
            if let JsonValue::Object(extra) = to_json(arg) {
                for (k, v) in extra {
                    map.insert(k, v);
                }
            }
        }
        return Value::from_serialize(base);
    }
    Value::from_serialize(base)
}

pub fn filter_dict_keys(value: Value, _args: Rest<Value>) -> Value {
    let j = to_json(&value);
    if let JsonValue::Object(map) = j {
        return Value::from_serialize(map.keys().cloned().collect::<Vec<String>>());
    }
    Value::from_serialize(Vec::<String>::new())
}

pub fn filter_dict_values(value: Value, _args: Rest<Value>) -> Value {
    let j = to_json(&value);
    if let JsonValue::Object(map) = j {
        return Value::from_serialize(map.values().cloned().collect::<Vec<JsonValue>>());
    }
    Value::from_serialize(Vec::<JsonValue>::new())
}

pub fn filter_dict_items(value: Value, _args: Rest<Value>) -> Value {
    let j = to_json(&value);
    if let JsonValue::Object(map) = j {
        let items: Vec<JsonValue> = map
            .into_iter()
            .map(|(k, v)| JsonValue::Array(vec![JsonValue::String(k), v]))
            .collect();
        return Value::from_serialize(items);
    }
    Value::from_serialize(Vec::<JsonValue>::new())
}

pub fn filter_zip_lists(value: Value, args: Rest<Value>) -> Value {
    let mut lists = Vec::new();
    if let Some(arr) = to_json(&value).as_array() {
        lists.push(arr.clone());
    }
    for arg in args.iter() {
        if let Some(arr) = to_json(arg).as_array() {
            lists.push(arr.clone());
        }
    }
    if lists.is_empty() {
        return Value::from_serialize(Vec::<JsonValue>::new());
    }
    let min_len = lists.iter().map(|l| l.len()).min().unwrap_or(0);
    let mut out = Vec::new();
    for i in 0..min_len {
        let tuple = lists
            .iter()
            .map(|l| l[i].clone())
            .collect::<Vec<JsonValue>>();
        out.push(JsonValue::Array(tuple));
    }
    Value::from_serialize(out)
}

pub fn filter_index_of(value: Value, args: Rest<Value>) -> Value {
    let j = to_json(&value);
    let needle = args.get(0).map(to_json).unwrap_or(JsonValue::Null);
    if let Some(arr) = j.as_array() {
        for (idx, item) in arr.iter().enumerate() {
            if item == &needle {
                return Value::from_serialize(idx as i64);
            }
        }
        return Value::from_serialize(-1);
    }
    Value::from_serialize(-1)
}

pub fn filter_intersection(value: Value, args: Rest<Value>) -> Value {
    let j = to_json(&value);
    let other = args.get(0).map(to_json).unwrap_or(JsonValue::Null);
    if let (Some(a), Some(b)) = (j.as_array(), other.as_array()) {
        let bset: HashSet<String> = b.iter().map(|v| v.to_string()).collect();
        let out: Vec<JsonValue> = a
            .iter()
            .filter(|v| bset.contains(&v.to_string()))
            .cloned()
            .collect();
        return Value::from_serialize(out);
    }
    Value::from_serialize(Vec::<JsonValue>::new())
}

pub fn filter_difference(value: Value, args: Rest<Value>) -> Value {
    let j = to_json(&value);
    let other = args.get(0).map(to_json).unwrap_or(JsonValue::Null);
    if let (Some(a), Some(b)) = (j.as_array(), other.as_array()) {
        let bset: HashSet<String> = b.iter().map(|v| v.to_string()).collect();
        let out: Vec<JsonValue> = a
            .iter()
            .filter(|v| !bset.contains(&v.to_string()))
            .cloned()
            .collect();
        return Value::from_serialize(out);
    }
    Value::from_serialize(Vec::<JsonValue>::new())
}

pub fn filter_union(value: Value, args: Rest<Value>) -> Value {
    let j = to_json(&value);
    let other = args.get(0).map(to_json).unwrap_or(JsonValue::Null);
    if let (Some(a), Some(b)) = (j.as_array(), other.as_array()) {
        let mut seen = HashSet::new();
        let mut out = Vec::new();
        for item in a.iter().chain(b.iter()) {
            let signature = item.to_string();
            if seen.insert(signature) {
                out.push(item.clone());
            }
        }
        return Value::from_serialize(out);
    }
    Value::from_serialize(Vec::<JsonValue>::new())
}

pub fn filter_format_json(value: Value, args: Rest<Value>) -> Value {
    let j = to_json(&value);
    let indent = args
        .get(0)
        .map(to_json)
        .and_then(|v| v.as_i64())
        .unwrap_or(2)
        .max(0) as usize;
    if indent == 0 {
        return Value::from_safe_string(serde_json::to_string(&j).unwrap_or_default());
    }
    Value::from_safe_string(serde_json::to_string_pretty(&j).unwrap_or_default())
}

pub fn filter_format_yaml(value: Value, _args: Rest<Value>) -> Value {
    let j = to_json(&value);
    Value::from_safe_string(serde_yaml::to_string(&j).unwrap_or_default())
}

pub fn filter_format_number(value: Value, args: Rest<Value>) -> Value {
    let j = to_json(&value);
    let decimals = args
        .get(0)
        .map(to_json)
        .and_then(|v| v.as_i64())
        .unwrap_or(2)
        .max(0) as usize;
    let sep = args
        .get(1)
        .map(to_json)
        .and_then(|v| v.as_str().map(|s| s.to_string()))
        .unwrap_or_else(|| ",".to_string());

    let number = if let Some(n) = j.as_i64() {
        n as f64
    } else if let Some(n) = j.as_f64() {
        n
    } else {
        return Value::from_safe_string(String::new());
    };

    let formatted = format!("{:.*}", decimals, number.abs());
    let mut parts = formatted.split('.');
    let int_part = parts.next().unwrap_or("0");
    let frac_part = parts.next();
    let int_value: i64 = int_part.parse().unwrap_or(0);
    let mut with_sep = int_value.to_formatted_string(&Locale::en);
    if sep != "," {
        with_sep = with_sep.replace(',', &sep);
    }
    let sign = if number < 0.0 { "-" } else { "" };
    let out = if let Some(frac) = frac_part {
        format!("{}{}.{}", sign, with_sep, frac)
    } else {
        format!("{}{}", sign, with_sep)
    };
    Value::from_safe_string(out)
}

pub fn filter_format_bytes(value: Value, args: Rest<Value>) -> Value {
    let j = to_json(&value);
    let precision = args
        .get(0)
        .map(to_json)
        .and_then(|v| v.as_i64())
        .unwrap_or(2)
        .max(0) as usize;
    let formatted = if let Some(n) = j.as_u64() {
        format_size(n, BINARY)
    } else if let Some(n) = j.as_i64() {
        if n >= 0 {
            format_size(n as u64, BINARY)
        } else {
            format!("-{}", format_size((-n) as u64, BINARY))
        }
    } else {
        String::new()
    };

    if formatted.is_empty() || precision == 2 {
        return Value::from_safe_string(formatted);
    }

    let parts: Vec<&str> = formatted.split_whitespace().collect();
    if parts.len() != 2 {
        return Value::from_safe_string(formatted);
    }
    if let Ok(value) = parts[0].parse::<f64>() {
        return Value::from_safe_string(format!("{:.*} {}", precision, value, parts[1]));
    }
    Value::from_safe_string(formatted)
}

pub fn filter_format_date(value: Value, args: Rest<Value>) -> Value {
    let fmt_json = args.get(0).map(|v| to_json(v)).unwrap_or(JsonValue::Null);
    let fmt = fmt_json.as_str().unwrap_or("%Y-%m-%d %H:%M:%S %z");
    let j = to_json(&value);
    if let Some(s) = j.as_str() {
        if let Ok(dt) = DateTime::parse_from_rfc3339(s) {
            return Value::from_safe_string(dt.format(fmt).to_string());
        }
        if let Ok(dt) = DateTime::parse_from_str(s, fmt) {
            return Value::from_safe_string(dt.format(fmt).to_string());
        }
    }
    if let Some(n) = j.as_i64() {
        if let Some(dt) = DateTime::<Utc>::from_timestamp(n, 0) {
            return Value::from_safe_string(dt.format(fmt).to_string());
        }
    }
    Value::from_safe_string(String::new())
}

pub fn filter_format_percentage(value: Value, args: Rest<Value>) -> Value {
    let j = to_json(&value);
    let decimals = args
        .get(0)
        .map(to_json)
        .and_then(|v| v.as_i64())
        .unwrap_or(2)
        .max(0) as usize;
    let multiply = args
        .get(1)
        .map(to_json)
        .and_then(|v| v.as_bool())
        .unwrap_or(true);
    if let Some(n) = j.as_f64().or_else(|| j.as_i64().map(|n| n as f64)) {
        let value = if multiply { n * 100.0 } else { n };
        return Value::from_safe_string(format!("{:.*}%", decimals, value));
    }
    Value::from_safe_string(String::new())
}

pub fn filter_format_currency(value: Value, args: Rest<Value>) -> Value {
    let j = to_json(&value);
    let symbol = args
        .get(0)
        .map(to_json)
        .and_then(|v| v.as_str().map(|s| s.to_string()))
        .unwrap_or_else(|| "$".to_string());
    let position = args
        .get(1)
        .map(to_json)
        .and_then(|v| v.as_str().map(|s| s.to_string()))
        .unwrap_or_else(|| "before".to_string());
    let formatted =
        filter_format_number(Value::from_serialize(j.clone()), Rest(Vec::new())).to_string();
    if position == "after" {
        Value::from_safe_string(format!("{}{}", formatted, symbol))
    } else {
        Value::from_safe_string(format!("{}{}", symbol, formatted))
    }
}

pub fn filter_format_ordinal(value: Value, _args: Rest<Value>) -> Value {
    let j = to_json(&value);
    let n = j.as_i64().unwrap_or(0);
    let suffix = if (10..=20).contains(&(n % 100)) {
        "th"
    } else {
        match n % 10 {
            1 => "st",
            2 => "nd",
            3 => "rd",
            _ => "th",
        }
    };
    Value::from_safe_string(format!("{}{}", n, suffix))
}

pub fn filter_format_phone(value: Value, args: Rest<Value>) -> Value {
    let j = to_json(&value);
    let format_str = args
        .get(0)
        .map(to_json)
        .and_then(|v| v.as_str().map(|s| s.to_string()))
        .unwrap_or_else(|| "({area}) {prefix}-{line}".to_string());
    let digits: String = as_str(&j).chars().filter(|c| c.is_ascii_digit()).collect();
    if digits.len() >= 10 {
        let area = &digits[digits.len() - 10..digits.len() - 7];
        let prefix = &digits[digits.len() - 7..digits.len() - 4];
        let line = &digits[digits.len() - 4..];
        return Value::from_safe_string(
            format_str
                .replace("{area}", area)
                .replace("{prefix}", prefix)
                .replace("{line}", line),
        );
    }
    Value::from_safe_string(as_str(&j).to_string())
}

pub fn filter_pad_left(value: Value, args: Rest<Value>) -> Value {
    let j = to_json(&value);
    let width = args
        .get(0)
        .map(to_json)
        .and_then(|v| v.as_i64())
        .unwrap_or(0)
        .max(0) as usize;
    let fill = args
        .get(1)
        .map(to_json)
        .and_then(|v| v.as_str().map(|s| s.chars().next().unwrap_or(' ')))
        .unwrap_or(' ');
    let s = as_str(&j);
    if s.len() >= width {
        return Value::from_safe_string(s.to_string());
    }
    let padding = fill.to_string().repeat(width - s.len());
    Value::from_safe_string(format!("{}{}", padding, s))
}

pub fn filter_pad_right(value: Value, args: Rest<Value>) -> Value {
    let j = to_json(&value);
    let width = args
        .get(0)
        .map(to_json)
        .and_then(|v| v.as_i64())
        .unwrap_or(0)
        .max(0) as usize;
    let fill = args
        .get(1)
        .map(to_json)
        .and_then(|v| v.as_str().map(|s| s.chars().next().unwrap_or(' ')))
        .unwrap_or(' ');
    let s = as_str(&j);
    if s.len() >= width {
        return Value::from_safe_string(s.to_string());
    }
    let padding = fill.to_string().repeat(width - s.len());
    Value::from_safe_string(format!("{}{}", s, padding))
}

pub fn filter_format_xml_escape(value: Value, _args: Rest<Value>) -> Value {
    let j = to_json(&value);
    let s = as_str(&j);
    let escaped = s
        .replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
        .replace('\'', "&apos;");
    Value::from_safe_string(escaped)
}

pub fn filter_format_sql_escape(value: Value, _args: Rest<Value>) -> Value {
    let j = to_json(&value);
    Value::from_safe_string(as_str(&j).replace('\'', "''"))
}

pub fn filter_tojson(value: Value, _args: Rest<Value>) -> Value {
    let j = to_json(&value);
    Value::from_safe_string(serde_json::to_string(&j).unwrap_or_default())
}

#[cfg(test)]
mod tests {
    use crate::engine::TemplateEngine;
    use serde_json::json;

    fn render(template: &str, data: serde_json::Value) -> String {
        let engine = TemplateEngine::new();
        engine.render_string(template, &data).unwrap()
    }

    #[test]
    fn test_filter_replace() {
        let res = render(
            "{{ 'hello world'|replace('world', 'templify') }}",
            json!({}),
        );
        assert_eq!(res, "hello templify");
    }

    #[test]
    fn test_filter_split_join() {
        let res = render("{{ 'a,b,c'|split(',')|join('|') }}", json!({}));
        assert_eq!(res, "a|b|c");
    }

    #[test]
    fn test_filter_pad_start_end() {
        let res = render("{{ '7'|pad_start(3, '0') }}", json!({}));
        assert_eq!(res, "007");
        let res = render("{{ '7'|pad_end(3, '0') }}", json!({}));
        assert_eq!(res, "700");
    }

    #[test]
    fn test_filter_capitalize_remove() {
        let res = render("{{ 'hello'|capitalize }}", json!({}));
        assert_eq!(res, "Hello");
        let res = render("{{ 'hello world'|remove('world') }}", json!({}));
        assert_eq!(res, "hello ");
    }

    #[test]
    fn test_filter_repeat_reverse() {
        let res = render("{{ 'ab'|repeat(3) }}", json!({}));
        assert_eq!(res, "ababab");
        let res = render("{{ [1,2,3]|reverse|join(',') }}", json!({}));
        assert_eq!(res, "3,2,1");
    }

    #[test]
    fn test_filter_truncate_slice() {
        let res = render("{{ 'hello world'|truncate(5, '...') }}", json!({}));
        assert_eq!(res, "hello...");
        let res = render("{{ 'templify'|slice(0, 4) }}", json!({}));
        assert_eq!(res, "temp");
        let res = render("{{ [1,2,3,4]|slice(1, 3)|join(',') }}", json!({}));
        assert_eq!(res, "2,3");
    }

    #[test]
    fn test_filter_length_first_last() {
        let res = render("{{ [1,2,3]|length }}", json!({}));
        assert_eq!(res, "3");
        let res = render("{{ 'templify'|length }}", json!({}));
        assert_eq!(res, "8");
        let res = render("{{ [1,2,3]|first }}", json!({}));
        assert_eq!(res, "1");
        let res = render("{{ [1,2,3]|last }}", json!({}));
        assert_eq!(res, "3");
    }

    #[test]
    fn test_filter_sum_min_max_round() {
        let res = render("{{ [1,2,3]|sum }}", json!({}));
        assert_eq!(res, "6");
        let res = render("{{ [1,2,3]|min }}", json!({}));
        assert_eq!(res, "1");
        let res = render("{{ [1,2,3]|max }}", json!({}));
        assert_eq!(res, "3");
        let res = render("{{ 3.14159|round(2) }}", json!({}));
        assert_eq!(res, "3.14");
    }

    #[test]
    fn test_filter_avg_median_unique_by_merge() {
        let res = render("{{ [1,2,3]|avg }}", json!({}));
        assert_eq!(res, "2");
        let res = render("{{ [1,2,3,4]|median }}", json!({}));
        assert_eq!(res, "2.5");
        let res = render(
            "{{ [{'id':1},{'id':1},{'id':2}]|unique_by('id')|length }}",
            json!({}),
        );
        assert_eq!(res, "2");
        let res = render("{{ {'a':1}|dict_merge({'b':2})|tojson }}", json!({}));
        assert!(res.contains("\"a\":1"));
        assert!(res.contains("\"b\":2"));
    }

    #[test]
    fn test_filter_string_extras() {
        let res = render("{{ 'line1\nline2'|indent_custom(2, true) }}", json!({}));
        assert_eq!(res, "  line1\n  line2");
        let res = render("{{ 'HelloWorld'|remove_prefix('Hello') }}", json!({}));
        assert_eq!(res, "World");
        let res = render("{{ 'file.txt'|remove_suffix('.txt') }}", json!({}));
        assert_eq!(res, "file");
        let res = render("{{ 'Hello World'|wrap_text(5) }}", json!({}));
        assert_eq!(res, "Hello\nWorld");
        let res = render("{{ 'Hello World'|truncate_custom(8, '>>') }}", json!({}));
        assert_eq!(res, "Hello >>");
        let res = render("{{ 'a1b2'|regex_search('\\\\d+') }}", json!({}));
        assert_eq!(res, "true");
        let res = render("{{ 'a1b2'|regex_findall('\\\\d')|join(',') }}", json!({}));
        assert_eq!(res, "1,2");
        let res = render("{{ 'hey'|quote_string }}", json!({}));
        assert_eq!(res, "\"hey\"");
    }

    #[test]
    fn test_filter_collection_extras() {
        let res = render("{{ [1,2,3,4]|chunk(2)|length }}", json!({}));
        assert_eq!(res, "2");
        let res = render("{{ {'a':1,'b':2}|dict_keys|join(',') }}", json!({}));
        assert_eq!(res, "a,b");
        let res = render("{{ {'a':1,'b':2}|dict_values|join(',') }}", json!({}));
        assert_eq!(res, "1,2");
        let res = render("{{ {'a':1,'b':2}|dict_items|length }}", json!({}));
        assert_eq!(res, "2");
        let res = render("{{ [1,2]|zip_lists(['a','b'])|length }}", json!({}));
        assert_eq!(res, "2");
        let res = render("{{ [1,2,3]|index_of(2) }}", json!({}));
        assert_eq!(res, "1");
        let res = render("{{ [1,2,3]|intersection([2,3,4])|join(',') }}", json!({}));
        assert_eq!(res, "2,3");
        let res = render("{{ [1,2,3]|difference([2,3,4])|join(',') }}", json!({}));
        assert_eq!(res, "1");
        let res = render("{{ [1,2,3]|union([3,4])|length }}", json!({}));
        assert_eq!(res, "4");
    }

    #[test]
    fn test_filter_formatting_extras() {
        let res = render("{{ 0.1234|format_percentage(2, true) }}", json!({}));
        assert_eq!(res, "12.34%");
        let res = render("{{ 1234.56|format_currency('$','before') }}", json!({}));
        assert!(res.contains("$"));
        let res = render("{{ 21|format_ordinal }}", json!({}));
        assert_eq!(res, "21st");
        let res = render("{{ '1234567890'|format_phone }}", json!({}));
        assert_eq!(res, "(123) 456-7890");
        let res = render("{{ '5'|pad_left(3,'0') }}", json!({}));
        assert_eq!(res, "005");
        let res = render("{{ '5'|pad_right(3,'0') }}", json!({}));
        assert_eq!(res, "500");
        let res = render("{{ '<tag>'|format_xml_escape }}", json!({}));
        assert_eq!(res, "&lt;tag&gt;");
        let res = render("{{ \"O'Brien\"|format_sql_escape }}", json!({}));
        assert_eq!(res, "O''Brien");
    }

    #[test]
    fn test_filter_utility_extras() {
        let res = render("{{ none|default_if_none('n/a') }}", json!({}));
        assert_eq!(res, "n/a");
        let res = render("{{ [1,2]|type_name }}", json!({}));
        assert_eq!(res, "list");
        let res = render("{{ [1,2]|is_list }}", json!({}));
        assert_eq!(res, "true");
        let res = render("{{ 2|is_even }}", json!({}));
        assert_eq!(res, "true");
        let res = render("{{ 'hello'|hash_sha256 }}", json!({}));
        assert!(res.starts_with("2cf24dba"));
        let res = render("{{ 'hello'|b64encode|b64decode }}", json!({}));
        assert_eq!(res, "hello");
        let res = render("{{ random_string(8, 'numeric')|length }}", json!({}));
        assert_eq!(res, "8");
        let res = render("{{ random_int(1,1) }}", json!({}));
        assert_eq!(res, "1");
        let res = render("{{ -5|abs_value }}", json!({}));
        assert_eq!(res, "5");
        let res = render("{{ 15|clamp(0,10) }}", json!({}));
        assert_eq!(res, "10");
        let res = render("{{ true|bool_to_string('yes','no') }}", json!({}));
        assert_eq!(res, "yes");
        let res = render("{{ 'path/to/file.txt'|file_extension }}", json!({}));
        assert_eq!(res, "txt");
        let res = render("{{ 'path/to/file.txt'|file_basename }}", json!({}));
        assert_eq!(res, "file.txt");
        let res = render("{{ 'path/to/file.txt'|file_dirname }}", json!({}));
        assert_eq!(res, "path/to");
        let res = render("{{ 10|safe_divide(2) }}", json!({}));
        assert_eq!(res, "5.0");
        let res = render("{{ 'red'|map_value({'red':'#f00'}, 'x') }}", json!({}));
        assert_eq!(res, "#f00");
        let res = render("{{ {'a':1}|get_attr('a') }}", json!({}));
        assert_eq!(res, "1");
        let res = render("{{ [9,8,7]|get_item(1) }}", json!({}));
        assert_eq!(res, "8");
    }

    #[test]
    fn test_filter_ternary() {
        let res = render("{{ true|ternary('yes', 'no') }}", json!({}));
        assert_eq!(res, "yes");
        let res = render("{{ false|ternary('yes', 'no') }}", json!({}));
        assert_eq!(res, "no");
    }

    #[test]
    fn test_filter_flatten() {
        let data = json!([1, [2, 3], 4]);
        let res = render("{{ data|flatten|tojson }}", json!({"data": data}));
        assert_eq!(res, "[1,2,3,4]");
    }

    #[test]
    fn test_filter_unique() {
        let data = json!([1, 2, 2, 3, 1]);
        let res = render("{{ data|unique|length }}", json!({"data": data}));
        assert_eq!(res, "3");
    }

    #[test]
    fn test_filter_pluck() {
        let data = json!([
            {"id": 1, "name": "A"},
            {"id": 2, "name": "B"},
        ]);
        let res = render("{{ data|pluck('name')|tojson }}", json!({"data": data}));
        assert_eq!(res, r#"["A","B"]"#);
    }

    #[test]
    fn test_filter_format_number() {
        let res = render("{{ 1234567|format_number }}", json!({}));
        assert_eq!(res, "1,234,567.00");
    }

    #[test]
    fn test_filter_uuid_generate() {
        let res = render("{{ 'test'|uuid_generate }}", json!({}));
        assert_eq!(res.len(), 36); // UUID length
        assert!(res.contains("-")); // Contains dashes
    }

    #[test]
    fn test_filter_regex_replace() {
        let res = render(
            "{{ 'hello world'|regex_replace('world', 'universe') }}",
            json!({}),
        );
        assert_eq!(res, "hello universe");
    }

    #[test]
    fn test_filter_coalesce() {
        let res = render("{{ null|coalesce('default') }}", json!({}));
        assert_eq!(res, "default");
        let res = render("{{ 'value'|coalesce('default') }}", json!({}));
        assert_eq!(res, "value");
    }

    #[test]
    fn test_filter_hash_md5() {
        let res = render("{{ 'hello'|hash_md5 }}", json!({}));
        assert_eq!(res, "5d41402abc4b2a76b9719d911017c592");
    }

    #[test]
    fn test_filter_compact() {
        let data = json!([1, null, 2, "", 3]);
        let res = render("{{ data|compact|length }}", json!({"data": data}));
        assert_eq!(res, "3");
    }

    #[test]
    fn test_filter_where() {
        let data = json!([
            {"name": "A", "active": true},
            {"name": "B", "active": false},
            {"name": "C", "active": true}
        ]);
        let res = render("{{ data|where('active')|length }}", json!({"data": data}));
        assert_eq!(res, "2");
    }

    #[test]
    fn test_filter_sort_by() {
        let data = json!([
            {"name": "C", "value": 3},
            {"name": "A", "value": 1},
            {"name": "B", "value": 2}
        ]);
        let res = render(
            "{{ (data|sort_by('name'))[0].name }}",
            json!({"data": data}),
        );
        assert_eq!(res, "A");
    }

    #[test]
    fn test_filter_group_by() {
        let data = json!([
            {"category": "A", "value": 1},
            {"category": "B", "value": 2},
            {"category": "A", "value": 3}
        ]);
        let res = render(
            "{{ (data|group_by('category')).A|length }}",
            json!({"data": data}),
        );
        assert_eq!(res, "2");
    }

    #[test]
    fn test_filter_format_json() {
        let res = render("{{ {'key': 'value'}|format_json }}", json!({}));
        assert!(res.contains("{\n"));
        assert!(res.contains("\"key\""));
    }

    #[test]
    fn test_filter_format_yaml() {
        let res = render("{{ {'key': 'value'}|format_yaml }}", json!({}));
        assert!(res.contains("key:"));
    }

    #[test]
    fn test_filter_format_bytes() {
        let res = render("{{ 1024|format_bytes }}", json!({}));
        assert_eq!(res, "1 KiB");
    }

    #[test]
    fn test_filter_format_date() {
        let res = render("{{ '2023-01-01T00:00:00Z'|format_date('%Y') }}", json!({}));
        assert_eq!(res, "2023");
    }
}
