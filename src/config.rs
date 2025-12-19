use serde::Deserialize;
use std::collections::HashMap;
use std::path::Path;
use thiserror::Error;

#[derive(Debug, Deserialize)]
pub struct TemplateConfig {
    pub globals: Option<HashMap<String, serde_json::Value>>,
    pub templates: Vec<TemplateSet>,
    #[serde(default = "default_flatten_data")]
    pub flatten_data: bool,
    
    #[serde(default)]
    pub manual_sections: ManualSectionConfig,

    #[serde(default)]
    pub extra_data: Vec<ExtraDataConfig>,
    
    #[serde(default)]
    pub format: FormatConfig,
}

fn default_flatten_data() -> bool {
    true
}

#[derive(Debug, Deserialize, Clone)]
pub struct ManualSectionConfig {
    #[serde(default = "default_manual_start")]
    pub start_marker: String,
    #[serde(default = "default_manual_end")]
    pub end_marker: String,
}

impl Default for ManualSectionConfig {
    fn default() -> Self {
        Self {
            start_marker: default_manual_start(),
            end_marker: default_manual_end(),
        }
    }
}

fn default_manual_start() -> String {
    "MANUAL SECTION START".to_string()
}

fn default_manual_end() -> String {
    "MANUAL SECTION END".to_string()
}

#[derive(Debug, Deserialize, Clone)]
pub struct ExtraDataConfig {
    pub key: String,
    pub path: String,
    #[serde(default)]
    pub required: bool,
}

#[derive(Debug, Deserialize, Default, Clone)]
pub struct FormatConfig {
    #[serde(default)]
    pub enabled: bool,
    #[serde(default)]
    pub formatters: HashMap<String, FormatterConfig>,
    #[serde(default)]
    pub defaults: FormatDefaults,
}

#[derive(Debug, Deserialize, Clone)]
pub struct FormatDefaults {
    #[serde(default)]
    pub ignore_patterns: Vec<String>,
    #[serde(default = "default_preserve_manual")]
    pub preserve_manual_sections: bool,
}

impl Default for FormatDefaults {
    fn default() -> Self {
        Self {
            ignore_patterns: Vec::new(),
            preserve_manual_sections: default_preserve_manual(),
        }
    }
}

fn default_preserve_manual() -> bool {
    true
}

#[derive(Debug, Deserialize, Clone)]
pub struct FormatterConfig {
    #[serde(rename = "type")]
    pub formatter_type: String, // e.g. "command"
    pub command: Option<String>,
    pub args: Option<Vec<String>>,
    #[serde(default)]
    pub options: HashMap<String, serde_json::Value>,
    #[serde(default = "default_enabled")]
    pub enabled: bool,
}

#[derive(Debug, Deserialize)]
pub struct TemplateSet {
    pub name: Option<String>,
    pub folder: String,
    pub output: Option<String>,
    pub iterate: Option<String>, // "item in items"
    #[serde(default = "default_enabled")]
    pub enabled: bool,
}

fn default_enabled() -> bool {
    true
}

#[derive(Debug)]
pub struct IterationInfo {
    pub var: String,
    pub expr: String,
}

#[derive(Error, Debug)]
pub enum ConfigError {
    #[error("Failed to read config file: {0}")]
    Io(#[from] std::io::Error),
    #[error("Failed to parse YAML: {0}")]
    Yaml(#[from] serde_yaml::Error),
    #[error("Invalid iteration syntax: {0}")]
    InvalidIteration(String),
}

impl TemplateConfig {
    pub fn load(path: &Path) -> Result<Self, ConfigError> {
        let content = std::fs::read_to_string(path)?;
        let config: TemplateConfig = serde_yaml::from_str(&content)?;
        Ok(config)
    }
}

// Moved parse_iteration logic to iteration.rs, but keeping a stub or moving it entirely?
// The plan says move it. So I'll remove it from here and put it in iteration.rs later.
// For now, I'll keep it to avoid breaking main.rs until I update it.
pub fn parse_iteration(iterate: &str) -> Result<IterationInfo, ConfigError> {
    let parts: Vec<&str> = iterate.split(" in ").collect();
    if parts.len() != 2 {
        return Err(ConfigError::InvalidIteration(iterate.to_string()));
    }
    Ok(IterationInfo {
        var: parts[0].trim().to_string(),
        expr: parts[1].trim().to_string(),
    })
}
