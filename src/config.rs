use jsonschema::JSONSchema;
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
    pub helpers: Vec<HelperConfig>,

    #[serde(default)]
    pub data_helpers: Option<DataHelpersConfig>,

    #[serde(default)]
    pub format: FormatConfig,

    #[serde(default)]
    pub validation: ValidationConfig,

    #[serde(default)]
    pub jinja_env: JinjaEnvConfig,

    #[serde(default = "default_template_suffixes")]
    pub template_suffixes: Vec<String>,

    pub schema: Option<String>,
}

fn default_flatten_data() -> bool {
    true
}

fn default_required_true() -> bool {
    true
}

fn default_auto_format() -> String {
    "auto".to_string()
}

fn default_template_suffixes() -> Vec<String> {
    vec![".j2".to_string(), ".jinja2".to_string(), ".inj".to_string()]
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

#[derive(Debug, Deserialize, Clone, Default)]
pub struct JinjaEnvConfig {
    #[serde(default)]
    pub trim_blocks: bool,
    #[serde(default)]
    pub lstrip_blocks: bool,
    #[serde(default)]
    pub autoescape: bool,
    #[serde(default = "default_keep_trailing_newline")]
    pub keep_trailing_newline: bool,
    pub newline_sequence: Option<String>,
    pub line_statement_prefix: Option<String>,
    pub line_comment_prefix: Option<String>,
}

fn default_keep_trailing_newline() -> bool {
    true
}

fn default_manual_start() -> String {
    "MANUAL SECTION START".to_string()
}

fn default_manual_end() -> String {
    "MANUAL SECTION END".to_string()
}

#[derive(Debug, Deserialize, Clone)]
#[serde(untagged)]
pub enum ExtraDataConfig {
    File(FileExtraDataConfig),
    Inline(InlineExtraDataConfig),
}

#[derive(Debug, Deserialize, Clone)]
pub struct FileExtraDataConfig {
    pub key: String,
    pub path: String,
    #[serde(default = "default_required_true")]
    pub required: bool,
    #[serde(default = "default_auto_format")]
    pub format: String,
    pub schema: Option<String>,
    pub description: Option<String>,
}

#[derive(Debug, Deserialize, Clone)]
pub struct InlineExtraDataConfig {
    pub key: String,
    pub value: serde_json::Value,
    pub schema: Option<String>,
    pub description: Option<String>,
}

#[derive(Debug, Deserialize, Clone)]
pub struct HelperConfig {
    pub key: String,
    pub path: String,
    #[serde(default = "default_auto_format")]
    pub format: String,
}

#[derive(Debug, Deserialize, Default, Clone)]
pub struct DataHelpersConfig {
    #[serde(default)]
    pub helpers: Vec<HelperConfig>,
    #[serde(default)]
    pub discovery_paths: Vec<String>,
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

#[derive(Debug, Deserialize, Default, Clone)]
pub struct ValidationConfig {
    #[serde(default)]
    pub validators: Vec<ValidatorSpec>,
}

#[derive(Debug, Deserialize, Clone)]
#[serde(tag = "type")]
pub enum ValidatorSpec {
    #[serde(rename = "file_structure")]
    FileStructure {
        name: Option<String>,
        #[serde(default)]
        paths: Vec<String>,
        #[serde(default)]
        patterns: Vec<String>,
        min: Option<usize>,
        max: Option<usize>,
    },
    #[serde(rename = "json_schema")]
    JsonSchema {
        name: Option<String>,
        schema: String,
        target: String,
    },
    #[serde(rename = "gtest")]
    Gtest {
        name: Option<String>,
        command: Option<String>,
        #[serde(default)]
        args: Vec<String>,
        working_dir: Option<String>,
    },
    #[serde(rename = "custom")]
    CustomCommand {
        name: Option<String>,
        command: String,
        #[serde(default)]
        args: Vec<String>,
        working_dir: Option<String>,
    },
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
    pub iterate: Option<IterationSpec>,
    #[serde(default = "default_enabled")]
    pub enabled: bool,
    #[serde(default)]
    pub files: FileFilters,
}

#[derive(Debug, Deserialize)]
#[serde(untagged)]
pub enum IterationSpec {
    Single(String),
    Multiple(Vec<String>),
}

#[derive(Debug, Deserialize, Default, Clone)]
pub struct FileFilters {
    #[serde(default)]
    pub include: Vec<String>,
    #[serde(default)]
    pub exclude: Vec<String>,
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
    #[error("Schema validation failed: {0}")]
    Schema(String),
}

impl TemplateConfig {
    pub fn load(path: &Path) -> Result<Self, ConfigError> {
        let content = std::fs::read_to_string(path)?;
        let raw: serde_yaml::Value = serde_yaml::from_str(&content)?;

        if let Some(schema_path) = raw.get("schema").and_then(|v| v.as_str()) {
            let schema_full = path.parent().unwrap_or(Path::new(".")).join(schema_path);
            let raw_json =
                serde_json::to_value(&raw).map_err(|e| ConfigError::Schema(e.to_string()))?;
            validate_against_schema(&raw_json, &schema_full)?;
        }

        let config: TemplateConfig = serde_yaml::from_value(raw)?;
        Ok(config)
    }
}

fn validate_against_schema(
    value: &serde_json::Value,
    schema_path: &Path,
) -> Result<(), ConfigError> {
    let schema_str =
        std::fs::read_to_string(schema_path).map_err(|e| ConfigError::Schema(e.to_string()))?;
    let schema_json: serde_json::Value =
        serde_json::from_str(&schema_str).map_err(|e| ConfigError::Schema(e.to_string()))?;

    let leaked: &'static serde_json::Value = Box::leak(Box::new(schema_json));
    let compiled = JSONSchema::compile(leaked).map_err(|e| ConfigError::Schema(e.to_string()))?;

    if let Err(errs) = compiled.validate(value) {
        let msgs: Vec<String> = errs.map(|e| e.to_string()).collect();
        return Err(ConfigError::Schema(msgs.join(", ")));
    }

    Ok(())
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

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;

    #[test]
    fn test_parse_iteration_valid() {
        let parsed = parse_iteration("item in items").unwrap();
        assert_eq!(parsed.var, "item");
        assert_eq!(parsed.expr, "items");
    }

    #[test]
    fn test_parse_iteration_invalid() {
        assert!(parse_iteration("item items").is_err());
        assert!(parse_iteration("a in b in c").is_err());
    }

    #[test]
    fn test_template_config_load_minimal() {
        let mut file = tempfile::NamedTempFile::new().unwrap();
        writeln!(file, "templates:\n  - folder: templates\n    enabled: true").unwrap();

        let config = TemplateConfig::load(file.path()).unwrap();
        assert_eq!(config.templates.len(), 1);
        assert!(config.flatten_data);
    }

    #[test]
    fn test_template_config_load_invalid_yaml() {
        let mut file = tempfile::NamedTempFile::new().unwrap();
        write!(file, "templates: [").unwrap();

        let result = TemplateConfig::load(file.path());
        assert!(result.is_err());
    }
}
