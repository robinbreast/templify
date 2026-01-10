use anyhow::{Context, Result};
use clap::{Parser, Subcommand};
use log::{info, warn};
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::process::Command;
use templify::config::{
    ExtraDataConfig, FileExtraDataConfig, FileFilters, IterationSpec, TemplateConfig, ValidatorSpec,
};
use templify::data_helpers::{collect_helpers, HelperDefs};
use templify::iteration::{IterationEvaluator, IterationInfo, IterationPattern};
use templify::manual_sections_cli::{run_manual_sections, ManualAction};
use templify::utils::{detect_format, expand_env_vars};
use templify::{FileGenerator, ManualSectionManager, TemplateEngine};

#[derive(Parser)]
#[command(author, version, about, long_about = None)]
struct Cli {
    #[command(subcommand)]
    command: Option<Commands>,

    /// Path to the YAML configuration file
    #[arg(short, long, global = true)]
    config: Option<PathBuf>,

    /// Path to the JSON data file
    #[arg(short, long, global = true)]
    data: Option<PathBuf>,

    /// Base output directory (overrides config if provided)
    #[arg(short, long, global = true)]
    output: Option<PathBuf>,

    /// Dry run mode - don't write files
    #[arg(long, global = true)]
    dry_run: bool,

    /// Include patterns (glob or regex:pattern)
    #[arg(long, global = true)]
    include: Vec<String>,

    /// Exclude patterns (glob or regex:pattern)
    #[arg(long, global = true)]
    exclude: Vec<String>,

    #[arg(long, global = true)]
    helper: Vec<String>,
}

#[derive(Subcommand)]
enum Commands {
    /// Initialize a new templify project
    Init {
        /// Project directory
        #[arg(default_value = ".")]
        path: PathBuf,
    },
    /// Generate files from templates (default command)
    Generate,
    ManualSections {
        #[command(subcommand)]
        action: ManualAction,
    },
    #[command(name = "manual-sections")]
    ManualSectionsAlias {
        #[command(subcommand)]
        action: ManualAction,
    },
}

fn main() -> Result<()> {
    env_logger::init();

    let cli = Cli::parse();

    match cli.command {
        Some(Commands::Init { path }) => {
            init_project(&path)?;
        }
        Some(Commands::ManualSections { ref action })
        | Some(Commands::ManualSectionsAlias { ref action }) => {
            handle_manual_sections(&cli, action.clone())?;
        }
        Some(Commands::Generate) | None => {
            generate(cli)?;
        }
    }

    Ok(())
}

fn init_project(path: &Path) -> Result<()> {
    info!("Initializing templify project at {:?}", path);

    // Create directory structure
    std::fs::create_dir_all(path.join("templates"))?;
    std::fs::create_dir_all(path.join("output"))?;

    // Create example config.yaml
    let config_content = r#"globals:
  version: "1.0.0"
  project: "MyProject"

manual_sections:
  start_marker: "MANUAL SECTION START"
  end_marker: "MANUAL SECTION END"

templates:
  - name: "Example"
    folder: "templates"
    output: "output"
    iterate: "item in items"
    enabled: true
"#;
    std::fs::write(path.join("config.yaml"), config_content)?;

    // Create example data.json
    let data_content = r#"{
  "items": [
    {"name": "item1", "value": 100},
    {"name": "item2", "value": 200}
  ]
}
"#;
    std::fs::write(path.join("data.json"), data_content)?;

    // Create example template
    let template_content = r#"# {{ item.name }}

Value: {{ item.value }}

MANUAL SECTION START: custom
# Add your custom content here
MANUAL SECTION END
"#;
    std::fs::write(
        path.join("templates/_foreach_item_{{ item.name }}.md.j2"),
        template_content,
    )?;

    info!("✓ Project initialized successfully!");
    info!("  Run: yagen -c config.yaml -d data.json");

    Ok(())
}

fn generate(cli: Cli) -> Result<()> {
    let config_path = cli
        .config
        .ok_or_else(|| anyhow::anyhow!("--config is required"))?;
    let data_path = cli
        .data
        .ok_or_else(|| anyhow::anyhow!("--data is required"))?;

    info!("Loading config from {:?}", config_path);
    let config = TemplateConfig::load(&config_path).context("Failed to load config")?;

    info!("Loading data from {:?}", data_path);
    let data_content = std::fs::read_to_string(&data_path).context("Failed to read data file")?;
    let data: serde_json::Value =
        serde_json::from_str(&data_content).context("Failed to parse JSON data")?;

    let output_base = cli
        .output
        .unwrap_or_else(|| config_path.parent().unwrap_or(Path::new(".")).to_path_buf());

    if cli.dry_run {
        info!("=== DRY RUN MODE ===");
    }

    let helper_defs =
        collect_helpers(&config, &cli.helper, &config_path).map_err(|e| anyhow::anyhow!(e))?;

    for template_set in &config.templates {
        if !template_set.enabled {
            continue;
        }

        if let Some(ref name) = template_set.name {
            if should_filter(name, &cli.include, &cli.exclude) {
                info!("Skipping template set: {}", name);
                continue;
            }
        }

        let template_folder = config_path
            .parent()
            .unwrap_or(Path::new("."))
            .join(&template_set.folder);

        let set_output_path = if let Some(ref out) = template_set.output {
            output_base.join(out)
        } else {
            output_base.clone()
        };

        let mut engine = TemplateEngine::with_env(config.jinja_env.clone());
        if let Some(ref globals) = config.globals {
            for (k, v) in globals {
                engine.add_global(k.clone(), v.clone());
            }
        }
        for (k, v) in &helper_defs.globals {
            engine.add_global(k.clone(), v.clone());
        }

        let manual_section_manager = ManualSectionManager::new(config.manual_sections.clone());
        let formatter_manager = templify::formatting::FormatterManager::new(
            config.format.clone(),
            manual_section_manager.clone(),
        );

        let generator = FileGenerator::new(engine, manual_section_manager, cli.dry_run)
            .with_template_suffixes(config.template_suffixes.clone())
            .with_formatter(formatter_manager);

        let files_filter = &template_set.files;

        if let Some(iterate_spec) = &template_set.iterate {
            let patterns = match iterate_spec {
                IterationSpec::Single(expr) => vec![IterationEvaluator::parse(expr)
                    .map_err(|e| anyhow::anyhow!("Failed to parse iteration: {}", e))?],
                IterationSpec::Multiple(list) => {
                    let mut out = Vec::new();
                    for expr in list {
                        out.push(
                            IterationEvaluator::parse(expr)
                                .map_err(|e| anyhow::anyhow!("Failed to parse iteration: {}", e))?,
                        );
                    }
                    out
                }
            };

            for pattern in patterns {
                process_iteration_pattern(
                    &generator,
                    &config,
                    &data,
                    &template_folder,
                    &set_output_path,
                    files_filter,
                    &pattern,
                    &config_path,
                    &helper_defs,
                )?;
            }
        } else {
            let context = build_base_context(
                &config,
                &data,
                &set_output_path,
                None,
                &config_path,
                &helper_defs,
            )?;

            generator
                .generate_filtered(&template_folder, &set_output_path, &context, files_filter)
                .map_err(|e| anyhow::anyhow!(e))?;
        }
    }

    if !cli.dry_run {
        run_validators(&config, &output_base, &config_path)?;
    }

    if cli.dry_run {
        info!("=== DRY RUN COMPLETE ===");
    }

    Ok(())
}

fn process_iteration_pattern(
    generator: &FileGenerator,
    config: &TemplateConfig,
    data: &serde_json::Value,
    template_folder: &Path,
    set_output_path: &Path,
    files_filter: &FileFilters,
    pattern: &IterationPattern,
    config_path: &Path,
    helper_defs: &HelperDefs,
) -> Result<()> {
    match pattern {
        IterationPattern::Simple(info) => process_simple_iteration(
            generator,
            config,
            data,
            template_folder,
            set_output_path,
            files_filter,
            info,
            config_path,
            helper_defs,
        )?,
        IterationPattern::Nested(list) => {
            let base_context = build_base_context(
                config,
                data,
                set_output_path,
                None,
                config_path,
                helper_defs,
            )?;
            process_nested_iteration(
                generator,
                config,
                data,
                template_folder,
                set_output_path,
                files_filter,
                list,
                0,
                base_context,
                config_path,
                helper_defs,
            )?;
        }
        IterationPattern::Array(array) => {
            for p in array {
                process_iteration_pattern(
                    generator,
                    config,
                    data,
                    template_folder,
                    set_output_path,
                    files_filter,
                    p,
                    config_path,
                    helper_defs,
                )?;
            }
        }
        IterationPattern::Union(union_patterns) => {
            for p in union_patterns {
                process_iteration_pattern(
                    generator,
                    config,
                    data,
                    template_folder,
                    set_output_path,
                    files_filter,
                    p,
                    config_path,
                    helper_defs,
                )?;
            }
        }
    }
    Ok(())
}

fn process_simple_iteration(
    generator: &FileGenerator,
    config: &TemplateConfig,
    data: &serde_json::Value,
    template_folder: &Path,
    set_output_path: &Path,
    files_filter: &FileFilters,
    info: &IterationInfo,
    config_path: &Path,
    helper_defs: &HelperDefs,
) -> Result<()> {
    let items = resolve_iterable(&info.expr, data, &HashMap::new()).ok_or_else(|| {
        anyhow::anyhow!(
            "Iteration expression '{}' did not resolve to an array",
            info.expr
        )
    })?;

    for item in items {
        let context = build_base_context(
            config,
            data,
            set_output_path,
            Some((info.var.as_str(), item.clone())),
            config_path,
            helper_defs,
        )?;

        if !condition_matches(&info.condition, &context) {
            continue;
        }

        generator
            .generate_filtered(template_folder, set_output_path, &context, files_filter)
            .map_err(|e| anyhow::anyhow!(e))?;
    }
    Ok(())
}

fn process_nested_iteration(
    generator: &FileGenerator,
    config: &TemplateConfig,
    data: &serde_json::Value,
    template_folder: &Path,
    set_output_path: &Path,
    files_filter: &FileFilters,
    list: &[IterationInfo],
    depth: usize,
    context: HashMap<String, serde_json::Value>,
    config_path: &Path,
    helper_defs: &HelperDefs,
) -> Result<()> {
    if depth >= list.len() {
        generator
            .generate_filtered(template_folder, set_output_path, &context, files_filter)
            .map_err(|e| anyhow::anyhow!(e))?;
        return Ok(());
    }

    let info = &list[depth];
    let items = resolve_iterable(&info.expr, data, &context).ok_or_else(|| {
        anyhow::anyhow!(
            "Iteration expression '{}' did not resolve to an array",
            info.expr
        )
    })?;

    for item in items {
        let mut next_context = context.clone();
        next_context.insert(info.var.clone(), item.clone());

        if !condition_matches(&info.condition, &next_context) {
            continue;
        }

        if depth + 1 == list.len() {
            generator
                .generate_filtered(
                    template_folder,
                    set_output_path,
                    &next_context,
                    files_filter,
                )
                .map_err(|e| anyhow::anyhow!(e))?;
        } else {
            process_nested_iteration(
                generator,
                config,
                data,
                template_folder,
                set_output_path,
                files_filter,
                list,
                depth + 1,
                next_context,
                config_path,
                helper_defs,
            )?;
        }
    }

    Ok(())
}

fn build_base_context(
    config: &TemplateConfig,
    data: &serde_json::Value,
    set_output_path: &Path,
    current_item: Option<(&str, serde_json::Value)>,
    config_path: &Path,
    helper_defs: &HelperDefs,
) -> Result<HashMap<String, serde_json::Value>> {
    let mut context = HashMap::new();

    if let Some(globals) = &config.globals {
        context.insert("globals".to_string(), serde_json::to_value(globals)?);
    }

    let output_dir_str = set_output_path.to_string_lossy().to_string();
    let mut globals_value = context
        .remove("globals")
        .unwrap_or_else(|| serde_json::Value::Object(serde_json::Map::new()));
    if let serde_json::Value::Object(ref mut map) = globals_value {
        map.insert(
            "output_dir".to_string(),
            serde_json::Value::String(output_dir_str.clone()),
        );
    }
    context.insert("globals".to_string(), globals_value.clone());
    context.insert("gg".to_string(), globals_value);
    context.insert(
        "output_dir".to_string(),
        serde_json::Value::String(output_dir_str),
    );

    context.insert("dd".to_string(), data.clone());

    for extra in &config.extra_data {
        match extra {
            ExtraDataConfig::File(file_cfg) => {
                if let Some(val) = load_extra_data_file(file_cfg, config_path)? {
                    context.insert(file_cfg.key.clone(), val);
                }
            }
            ExtraDataConfig::Inline(inline_cfg) => {
                if let Some(schema) = &inline_cfg.schema {
                    validate_with_schema(
                        &inline_cfg.value,
                        schema,
                        config_path.parent().unwrap_or(Path::new(".")),
                    )?;
                }
                context.insert(inline_cfg.key.clone(), inline_cfg.value.clone());
            }
        }
    }

    if config.flatten_data {
        if let serde_json::Value::Object(map) = data {
            for (k, v) in map {
                context.insert(k.clone(), v.clone());
            }
        }
    }

    for (k, v) in &helper_defs.context_entries {
        context.insert(k.clone(), v.clone());
    }

    if let Some((key, val)) = current_item {
        context.insert(key.to_string(), val);
    }

    Ok(context)
}

fn load_extra_data_file(
    file_cfg: &FileExtraDataConfig,
    config_path: &Path,
) -> Result<Option<serde_json::Value>> {
    let base = config_path.parent().unwrap_or(Path::new("."));
    let expanded_path = expand_env_vars(&file_cfg.path);
    let extra_path = base.join(expanded_path);
    let format = detect_format(&file_cfg.format, &extra_path);

    let content = match std::fs::read_to_string(&extra_path) {
        Ok(c) => c,
        Err(_) => {
            if file_cfg.required {
                return Err(anyhow::anyhow!(
                    "Required extra data file not found: {:?}",
                    extra_path
                ));
            }
            warn!("Optional extra data file not found: {:?}", extra_path);
            return Ok(None);
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

    if let Some(schema_path) = &file_cfg.schema {
        if let Some(val) = &parsed {
            validate_with_schema(val, schema_path, base)?;
        }
    }

    if parsed.is_none() {
        if file_cfg.required {
            return Err(anyhow::anyhow!(
                "Required extra data file failed to parse: {:?}",
                extra_path
            ));
        }
        warn!("Failed to parse extra data from {:?}", extra_path);
    }

    Ok(parsed)
}

fn validate_with_schema(value: &serde_json::Value, schema_path: &str, base: &Path) -> Result<()> {
    let expanded_schema = expand_env_vars(schema_path);
    let schema_full = base.join(expanded_schema);
    let schema_str = std::fs::read_to_string(&schema_full)
        .with_context(|| format!("Failed to read schema file: {:?}", schema_full))?;
    let schema_json: serde_json::Value = serde_json::from_str(&schema_str)
        .with_context(|| format!("Failed to parse schema JSON: {:?}", schema_full))?;
    let schema_leaked: &'static serde_json::Value = Box::leak(Box::new(schema_json));
    let compiled = jsonschema::JSONSchema::compile(schema_leaked)
        .with_context(|| format!("Invalid JSON Schema: {:?}", schema_full))?;
    if let Err(errs) = compiled.validate(value) {
        let msgs: Vec<String> = errs.map(|e| e.to_string()).collect();
        return Err(anyhow::anyhow!(
            "Schema validation failed for {:?}: {}",
            schema_full,
            msgs.join(", ")
        ));
    }
    Ok(())
}

fn run_validators(config: &TemplateConfig, output_base: &Path, config_path: &Path) -> Result<()> {
    if config.validation.validators.is_empty() {
        return Ok(());
    }

    let base = config_path.parent().unwrap_or(Path::new("."));
    let mut errors = Vec::new();

    for validator in &config.validation.validators {
        match validator {
            ValidatorSpec::FileStructure {
                name,
                paths,
                patterns,
                min,
                max,
            } => {
                let label = name.clone().unwrap_or_else(|| "file_structure".to_string());
                let mut missing = Vec::new();

                for rel_path in paths {
                    let resolved = expand_env_vars(rel_path);
                    let path = output_base.join(resolved);
                    if !path.exists() {
                        missing.push(rel_path.clone());
                    }
                }

                let mut matched_paths = std::collections::HashSet::new();
                for pattern in patterns {
                    let resolved = expand_env_vars(pattern);
                    let glob_path = output_base.join(resolved);
                    let pattern_str = glob_path.to_string_lossy().to_string();
                    for entry in glob::glob(&pattern_str)
                        .with_context(|| format!("Invalid glob pattern: {}", pattern_str))?
                    {
                        if let Ok(path) = entry {
                            matched_paths.insert(path);
                        }
                    }
                }

                let total_matches = matched_paths.len();
                if !patterns.is_empty() && total_matches == 0 && min.is_none() && max.is_none() {
                    errors.push(format!("Validator '{}' matched no files", label));
                }
                if let Some(min) = min {
                    if total_matches < *min {
                        errors.push(format!(
                            "Validator '{}' expected at least {} match(es), found {}",
                            label, min, total_matches
                        ));
                    }
                }
                if let Some(max) = max {
                    if total_matches > *max {
                        errors.push(format!(
                            "Validator '{}' expected at most {} match(es), found {}",
                            label, max, total_matches
                        ));
                    }
                }

                if !missing.is_empty() {
                    errors.push(format!(
                        "Validator '{}' missing paths: {}",
                        label,
                        missing.join(", ")
                    ));
                }
            }
            ValidatorSpec::JsonSchema {
                name,
                schema,
                target,
            } => {
                let schema_path = base.join(expand_env_vars(schema));
                let target_path = output_base.join(expand_env_vars(target));
                let label = name.clone().unwrap_or_else(|| "json_schema".to_string());
                let content = std::fs::read_to_string(&target_path).map_err(|e| {
                    anyhow::anyhow!(
                        "Validator '{}' failed to read target {:?}: {}",
                        label,
                        target_path,
                        e
                    )
                })?;
                let format = detect_format("auto", &target_path);
                let parsed = match format.as_str() {
                    "yaml" | "yml" => serde_yaml::from_str(&content).ok(),
                    "json" => serde_json::from_str(&content).ok(),
                    "toml" => toml::from_str::<toml::Value>(&content)
                        .ok()
                        .and_then(|v| serde_json::to_value(v).ok()),
                    _ => serde_json::from_str(&content).ok(),
                };
                let Some(value) = parsed else {
                    return Err(anyhow::anyhow!(
                        "Validator '{}' failed to parse target {:?}",
                        label,
                        target_path
                    ));
                };
                validate_with_schema(&value, schema_path.to_str().unwrap_or(""), base)
                    .map_err(|e| anyhow::anyhow!("Validator '{}' failed: {}", label, e))?;
            }
            ValidatorSpec::Gtest {
                name,
                command,
                args,
                working_dir,
            } => {
                let label = name.clone().unwrap_or_else(|| "gtest".to_string());
                let cmd = command.as_deref().unwrap_or("ctest");
                run_command_validator(&label, cmd, args, working_dir, output_base)?;
            }
            ValidatorSpec::CustomCommand {
                name,
                command,
                args,
                working_dir,
            } => {
                let label = name.clone().unwrap_or_else(|| "custom".to_string());
                run_command_validator(&label, command, args, working_dir, output_base)?;
            }
        }
    }

    if !errors.is_empty() {
        return Err(anyhow::anyhow!(errors.join("\n")));
    }

    Ok(())
}

fn run_command_validator(
    label: &str,
    command: &str,
    args: &[String],
    working_dir: &Option<String>,
    output_base: &Path,
) -> Result<()> {
    let mut cmd = Command::new(command);
    cmd.args(args);

    if let Some(dir) = working_dir {
        let resolved = expand_env_vars(dir);
        let path = output_base.join(resolved);
        cmd.current_dir(path);
    } else {
        cmd.current_dir(output_base);
    }

    let status = cmd.status().with_context(|| {
        format!(
            "Validator '{}' failed to launch command: {}",
            label, command
        )
    })?;

    if !status.success() {
        return Err(anyhow::anyhow!(
            "Validator '{}' command failed with status: {}",
            label,
            status
        ));
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::env;
    use templify::utils::{expand_env_vars, insert_nested_value};
    use templify::ManualSectionConfig;

    #[test]
    fn test_expand_env_vars() {
        env::set_var("TEMPLIFY_TEST_VAR", "value");
        let input = "${TEMPLIFY_TEST_VAR}/path/%TEMPLIFY_TEST_VAR%";
        let expanded = expand_env_vars(input);
        assert_eq!(expanded, "value/path/value");
        env::remove_var("TEMPLIFY_TEST_VAR");
    }

    #[test]
    fn test_insert_nested_value() {
        let mut target = HashMap::new();
        insert_nested_value(
            &mut target,
            "utils.string",
            serde_json::json!({"name": "templify"}),
        );

        let utils = target.get("utils").unwrap();
        let utils_obj = utils.as_object().unwrap();
        assert!(utils_obj.contains_key("string"));
    }

    #[test]
    fn test_run_validators_file_structure() {
        let temp_dir = tempfile::tempdir().unwrap();
        let output_base = temp_dir.path();
        std::fs::create_dir_all(output_base.join("src")).unwrap();
        std::fs::write(output_base.join("src/main.rs"), "fn main() {}\n").unwrap();

        let config = TemplateConfig {
            globals: None,
            templates: Vec::new(),
            flatten_data: true,
            manual_sections: ManualSectionConfig::default(),
            extra_data: Vec::new(),
            helpers: Vec::new(),
            data_helpers: None,
            format: templify::config::FormatConfig::default(),
            validation: templify::config::ValidationConfig {
                validators: vec![ValidatorSpec::FileStructure {
                    name: Some("check".to_string()),
                    paths: vec!["src/main.rs".to_string()],
                    patterns: vec!["src/*.rs".to_string()],
                    min: Some(1),
                    max: None,
                }],
            },
            jinja_env: templify::config::JinjaEnvConfig::default(),
            template_suffixes: vec![".j2".to_string()],
            schema: None,
        };

        let config_path = output_base.join("config.yaml");
        std::fs::write(&config_path, "templates: []").unwrap();

        let result = run_validators(&config, output_base, &config_path);
        assert!(result.is_ok());
    }
}

fn condition_matches(
    condition: &Option<String>,
    context: &HashMap<String, serde_json::Value>,
) -> bool {
    if condition.is_none() {
        return true;
    }
    let ctx_value = serde_json::to_value(context).unwrap_or(serde_json::Value::Null);
    let ptr = format!("/{}", condition.as_ref().unwrap().replace('.', "/"));
    match ctx_value.pointer(&ptr) {
        Some(serde_json::Value::Bool(b)) => *b,
        Some(serde_json::Value::Null) => false,
        Some(_) => true,
        None => false,
    }
}

fn resolve_iterable(
    expr: &str,
    data: &serde_json::Value,
    context: &HashMap<String, serde_json::Value>,
) -> Option<Vec<serde_json::Value>> {
    let path = IterationEvaluator::evaluate_path(expr);
    if let Some(val) = data.pointer(&path) {
        if let serde_json::Value::Array(arr) = val {
            return Some(arr.clone());
        }
    }

    let ctx_value = serde_json::to_value(context).ok()?;
    if let Some(val) = ctx_value.pointer(&path) {
        if let serde_json::Value::Array(arr) = val {
            return Some(arr.clone());
        }
    }

    None
}

fn handle_manual_sections(cli: &Cli, action: ManualAction) -> Result<()> {
    run_manual_sections(cli.config.clone(), action)
}

fn should_filter(name: &str, include: &[String], exclude: &[String]) -> bool {
    // If include patterns are specified, name must match at least one
    if !include.is_empty() {
        let mut matched = false;
        for pattern in include {
            if matches_pattern(name, pattern) {
                matched = true;
                break;
            }
        }
        if !matched {
            return true; // Filter out
        }
    }

    // If exclude patterns are specified, name must not match any
    for pattern in exclude {
        if matches_pattern(name, pattern) {
            return true; // Filter out
        }
    }

    false // Don't filter
}

fn matches_pattern(name: &str, pattern: &str) -> bool {
    if let Some(regex_pattern) = pattern.strip_prefix("regex:") {
        if let Ok(re) = regex::Regex::new(regex_pattern) {
            return re.is_match(name);
        }
    }

    // Simple glob-like matching (very basic)
    if pattern.contains('*') {
        let parts: Vec<&str> = pattern.split('*').collect();
        if parts.len() == 2 {
            return name.starts_with(parts[0]) && name.ends_with(parts[1]);
        }
    }

    name == pattern
}
