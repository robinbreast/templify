use chrono::{DateTime, Utc};
use heck::{ToKebabCase, ToLowerCamelCase, ToSnakeCase, ToUpperCamelCase};
use humansize::{format_size, BINARY};
use md5;
use minijinja::value::{Rest, Value};
use num_format::{Locale, ToFormattedString};
use regex::Regex;
use serde_json::Value as JsonValue;
use std::collections::{HashMap, HashSet};
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

pub fn filter_uuid_generate(value: Value, _args: Rest<Value>) -> Value {
    let j = to_json(&value);
    if let Some(s) = j.as_str() {
        Value::from_safe_string(Uuid::new_v5(&Uuid::NAMESPACE_OID, s.as_bytes()).to_string())
    } else {
        Value::from_safe_string(Uuid::new_v4().to_string())
    }
}

pub fn filter_regex_replace(value: Value, args: Rest<Value>) -> Value {
    let j = to_json(&value);
    let s = as_str(&j);
    let pattern_json = args.get(0).map(|v| to_json(v)).unwrap_or(JsonValue::Null);
    let repl_json = args.get(1).map(|v| to_json(v)).unwrap_or(JsonValue::Null);
    let pattern = pattern_json.as_str().unwrap_or("");
    let repl = repl_json.as_str().unwrap_or("");
    let re = Regex::new(pattern).unwrap_or_else(|_| Regex::new("").unwrap());
    Value::from_safe_string(re.replace_all(s, repl).to_string())
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

pub fn filter_hash_md5(value: Value, _args: Rest<Value>) -> Value {
    let j = to_json(&value);
    let s = as_str(&j);
    let digest = md5::compute(s.as_bytes());
    Value::from_safe_string(format!("{:x}", digest))
}

pub fn filter_flatten(value: Value, _args: Rest<Value>) -> Value {
    let j = to_json(&value);
    let mut out = Vec::new();
    if let Some(arr) = j.as_array() {
        for v in arr {
            if let Some(inner) = v.as_array() {
                for iv in inner {
                    out.push(iv.clone());
                }
            } else {
                out.push(v.clone());
            }
        }
    }
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

pub fn filter_format_json(value: Value, _args: Rest<Value>) -> Value {
    let j = to_json(&value);
    Value::from_safe_string(serde_json::to_string_pretty(&j).unwrap_or_default())
}

pub fn filter_format_yaml(value: Value, _args: Rest<Value>) -> Value {
    let j = to_json(&value);
    Value::from_safe_string(serde_yaml::to_string(&j).unwrap_or_default())
}

pub fn filter_format_number(value: Value, _args: Rest<Value>) -> Value {
    let j = to_json(&value);
    if let Some(n) = j.as_i64() {
        Value::from_safe_string(n.to_formatted_string(&Locale::en))
    } else if let Some(n) = j.as_f64() {
        Value::from_safe_string(format!("{}", n))
    } else {
        Value::from_safe_string(String::new())
    }
}

pub fn filter_format_bytes(value: Value, _args: Rest<Value>) -> Value {
    let j = to_json(&value);
    if let Some(n) = j.as_u64() {
        Value::from_safe_string(format_size(n, BINARY))
    } else if let Some(n) = j.as_i64() {
        if n >= 0 {
            Value::from_safe_string(format_size(n as u64, BINARY))
        } else {
            Value::from_safe_string(format!("-{}", format_size((-n) as u64, BINARY)))
        }
    } else {
        Value::from_safe_string(String::new())
    }
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

pub fn filter_tojson(value: Value, _args: Rest<Value>) -> Value {
    let j = to_json(&value);
    Value::from_safe_string(serde_json::to_string(&j).unwrap_or_default())
}

#[cfg(test)]
mod tests {
    use crate::engine::TemplateEngine;
    use serde_json::json;
    use std::collections::HashMap;

    fn render(template: &str, data: serde_json::Value) -> String {
        let engine = TemplateEngine::new();
        engine.render_string(template, &data).unwrap()
    }

    #[test]
    fn test_filter_camelcase() {
        let res = render("{{ 'hello_world'|camelcase }}", json!({}));
        assert_eq!(res, "helloWorld");
    }

    #[test]
    fn test_filter_pascalcase() {
        let res = render("{{ 'hello_world'|pascalcase }}", json!({}));
        assert_eq!(res, "HelloWorld");
    }

    #[test]
    fn test_filter_snakecase() {
        let res = render("{{ 'helloWorld'|snakecase }}", json!({}));
        assert_eq!(res, "hello_world");
    }

    #[test]
    fn test_filter_kebabcase() {
        let res = render("{{ 'hello_world'|kebabcase }}", json!({}));
        assert_eq!(res, "hello-world");
    }

    #[test]
    fn test_filter_screamingsnakecase() {
        let res = render("{{ 'hello_world'|screamingsnakecase }}", json!({}));
        assert_eq!(res, "HELLO_WORLD");
    }

    #[test]
    fn test_filter_slugify() {
        let res = render("{{ 'Hello World!'|slugify }}", json!({}));
        assert_eq!(res, "hello-world");
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
        assert_eq!(res, "1,234,567");
    }
}
