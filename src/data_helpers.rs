//! Data helper loading and orchestration.
//!
//! This module provides the helper discovery and loading system inspired by `pytemplify`.
//! Helpers are external data files (JSON, YAML, TOML) that get loaded into the template context.
//!
//! Features:
//! - Explicit helper definitions via config or CLI
//! - Glob-based discovery across multiple search paths
//! - Dot-notation namespacing for nested context injection

use crate::config::{HelperConfig, TemplateConfig};
use crate::utils::{detect_format, expand_env_vars, insert_nested_value};
use log::warn;
use serde_json::Value;
use std::collections::HashMap;
use std::path::{Path, PathBuf};

/// Resolved helper data ready for template integration.
#[derive(Debug, Default, Clone)]
pub struct HelperDefs {
    /// Data registered as Jinja globals.
    pub globals: HashMap<String, Value>,
    /// Data merged into the template context.
    pub context_entries: HashMap<String, Value>,
}

/// Collects helpers from config, CLI args, and discovery paths.
///
/// Loads data files, resolves globs, and builds the `HelperDefs` structure.
pub fn collect_helpers(
    config: &TemplateConfig,
    cli_helpers: &[String],
    config_path: &Path,
) -> Result<HelperDefs, String> {
    let mut entries = HashMap::new();
    let mut globals = HashMap::new();

    let mut helpers: Vec<HelperConfig> = config.helpers.clone();
    let mut discovery_paths: Vec<String> = Vec::new();

    if let Some(data_helpers) = &config.data_helpers {
        helpers.extend(data_helpers.helpers.clone());
        discovery_paths.extend(data_helpers.discovery_paths.clone());
    }

    for raw in cli_helpers {
        helpers.push(parse_helper_arg(raw)?);
    }

    let base = config_path.parent().unwrap_or(Path::new("."));
    let search_paths = build_search_paths(base, &discovery_paths);

    for helper in &helpers {
        if helper.path.contains('*') {
            let pattern_paths = resolve_helper_glob_paths(base, &search_paths, &helper.path);
            for pattern_str in pattern_paths {
                let entries_iter = glob::glob(&pattern_str)
                    .map_err(|e| format!("Failed to read glob pattern {}: {}", pattern_str, e))?;
                for entry in entries_iter {
                    match entry {
                        Ok(path) => {
                            if path.is_file() {
                                let stem = path.file_stem().unwrap().to_string_lossy().to_string();
                                let sub_key = if helper.key.is_empty() {
                                    stem
                                } else {
                                    format!("{}.{}", helper.key, stem)
                                };

                                let sub_helper = HelperConfig {
                                    key: sub_key.clone(),
                                    path: path.to_string_lossy().to_string(),
                                    format: helper.format.clone(),
                                };

                                if let Some(val) =
                                    load_helper_file(&sub_helper, base, &search_paths)?
                                {
                                    insert_nested_value(&mut entries, &sub_key, val.clone());
                                    if helper.key.is_empty() {
                                        insert_nested_value(&mut globals, &sub_key, val);
                                    }
                                }
                            }
                        }
                        Err(e) => warn!("Glob error: {}", e),
                    }
                }
            }
        } else if let Some(val) = load_helper_file(helper, base, &search_paths)? {
            insert_nested_value(&mut entries, &helper.key, val.clone());
            insert_nested_value(&mut globals, &helper.key, val);
        }
    }

    Ok(HelperDefs {
        globals,
        context_entries: entries,
    })
}

/// Parses a CLI helper argument in `key=path` format.
pub fn parse_helper_arg(raw: &str) -> Result<HelperConfig, String> {
    let parts: Vec<&str> = raw.splitn(2, '=').collect();
    if parts.len() != 2 {
        return Err("--helper expects key=path".to_string());
    }
    Ok(HelperConfig {
        key: parts[0].to_string(),
        path: parts[1].to_string(),
        format: "auto".to_string(),
    })
}

fn build_search_paths(base: &Path, discovery_paths: &[String]) -> Vec<PathBuf> {
    let mut paths = Vec::new();
    for path in discovery_paths {
        let expanded = expand_env_vars(path);
        let resolved = base.join(expanded);
        paths.push(resolved);
    }
    paths
}

fn resolve_helper_path(base: &Path, search_paths: &[PathBuf], path: &str) -> PathBuf {
    let candidate = Path::new(path);
    if candidate.is_absolute() {
        return candidate.to_path_buf();
    }

    let base_candidate = base.join(path);
    if base_candidate.exists() {
        return base_candidate;
    }

    for search in search_paths {
        let candidate = search.join(path);
        if candidate.exists() {
            return candidate;
        }
    }

    base.join(path)
}

fn resolve_helper_glob_paths(base: &Path, search_paths: &[PathBuf], path: &str) -> Vec<String> {
    let candidate = Path::new(path);
    if candidate.is_absolute() {
        return vec![candidate.to_string_lossy().to_string()];
    }

    let mut patterns = Vec::new();
    patterns.push(base.join(path).to_string_lossy().to_string());
    for search in search_paths {
        patterns.push(search.join(path).to_string_lossy().to_string());
    }
    patterns
}

fn load_helper_file(
    helper: &HelperConfig,
    base: &Path,
    search_paths: &[PathBuf],
) -> Result<Option<Value>, String> {
    let expanded_path = expand_env_vars(&helper.path);
    let helper_path = resolve_helper_path(base, search_paths, &expanded_path);
    let format = detect_format(&helper.format, &helper_path);
    let content = match std::fs::read_to_string(&helper_path) {
        Ok(c) => c,
        Err(_) => {
            return Err(format!("Helper file not found: {:?}", helper_path));
        }
    };

    let parsed = match format.as_str() {
        "yaml" | "yml" => serde_yaml::from_str(&content).ok(),
        "json" => serde_json::from_str(&content).ok(),
        "toml" => toml::from_str::<toml::Value>(&content)
            .ok()
            .and_then(|v| serde_json::to_value(v).ok()),
        _ => serde_json::from_str(&content).ok(),
    };

    Ok(parsed)
}
