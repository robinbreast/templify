use anyhow::{Context, Result};
use clap::{Parser, Subcommand};
use log::{info, warn};
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use templify::config::{
    ExtraDataConfig, FileExtraDataConfig, FileFilters, HelperConfig, IterationSpec, TemplateConfig,
};
use templify::iteration::{IterationEvaluator, IterationInfo, IterationPattern};
use templify::{FileGenerator, ManualSectionConfig, ManualSectionManager, TemplateEngine};

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
}

#[derive(Subcommand, Clone)]
enum ManualAction {
    Backup {
        #[arg(long)]
        input: PathBuf,
        #[arg(long)]
        backup: PathBuf,
    },
    Restore {
        #[arg(long)]
        input: PathBuf,
        #[arg(long)]
        backup: PathBuf,
        #[arg(long)]
        output: Option<PathBuf>,
    },
}

fn main() -> Result<()> {
    env_logger::init();

    let cli = Cli::parse();

    match cli.command {
        Some(Commands::Init { path }) => {
            init_project(&path)?;
        }
        Some(Commands::ManualSections { ref action }) => {
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

    let helper_defs = collect_helpers(&config, &cli.helper, &config_path)?;

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
    context.insert("globals".to_string(), globals_value);
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
    let extra_path = base.join(&file_cfg.path);
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
    let schema_full = base.join(schema_path);
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

fn detect_format(format: &str, path: &Path) -> String {
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

struct HelperDefs {
    globals: HashMap<String, serde_json::Value>,
    context_entries: HashMap<String, serde_json::Value>,
}

fn collect_helpers(
    config: &TemplateConfig,
    cli_helpers: &[String],
    config_path: &Path,
) -> Result<HelperDefs> {
    let mut entries = HashMap::new();
    let mut globals = HashMap::new();

    let mut helpers: Vec<HelperConfig> = config.helpers.clone();
    for raw in cli_helpers {
        helpers.push(parse_helper_arg(raw)?);
    }

    let base = config_path.parent().unwrap_or(Path::new("."));

    for helper in &helpers {
        if helper.path.contains('*') {
            // Glob pattern
            let pattern = base.join(&helper.path);
            let pattern_str = pattern.to_string_lossy();
            for entry in glob::glob(&pattern_str).context("Failed to read glob pattern")? {
                match entry {
                    Ok(path) => {
                        if path.is_file() {
                            // Synthesize a single-file config for loading
                            // The key needs to be derived or we use the base key?
                            // Strategy: if it's a glob, 'key' might be a prefix or we use the filename stem.
                            // If user said "myhelper=helpers/*.json", loading "helpers/foo.json" as "myhelper" overwrites?
                            // Python behavior: recursive load merges into smart dict.
                            // Here we'll try to use filename stem as sub-key if possible, or just merge?
                            // For simplicity, let's load into a map under the 'helper.key' if it's a map?
                            // Actually, let's assume one helper file = one entry in context.
                            // If glob, we probably want: key="data" path="*.json" -> data.foo, data.bar?
                            // Current implementation of 'load_helper_file' returns a Value.
                            // If we have multiple files, we should probably merge them or error?
                            // Let's do: key is a namespace.
                            // If key is "mydata", and we find foo.json, we insert at mydata.foo?
                            // Or just insert each file at its stem name?
                            // Let's stick to simple: if path has glob, we ignore the key and use file stems?
                            // Or respect key as prefix?
                            // Pytemplify uses `helpers` list where you define `variable: path`.
                            // If path is glob, it loads all matches.
                            // If `variable` is "utils", and we load `string.py`, it becomes `utils.string`.
                            // So let's implement that: key is the namespace.

                            let stem = path.file_stem().unwrap().to_string_lossy().to_string();
                            let sub_key = if helper.key.is_empty() {
                                stem
                            } else {
                                format!("{}.{}", helper.key, stem)
                            };

                            // Re-use load logic by making a temp config
                            let sub_helper = HelperConfig {
                                key: sub_key.clone(),
                                path: path.to_string_lossy().to_string(), // Absolute path from glob
                                format: helper.format.clone(),
                            };

                            // We pass absolute path, so base can be root? Or just use parent?
                            // load_helper_file joins base+path. If path is absolute, join works (ignores base).
                            if let Some(val) = load_helper_file(&sub_helper, Path::new(""))? {
                                // Insert into entries (support dotted keys? context currently is HashMap<String, Value>)
                                // context only supports top-level keys.
                                // If we want `utils.string`, we need to structure the HashMap.
                                // But `collect_helpers` returns flat map.
                                // minijinja context keys with dots are invalid identifiers usually?
                                // Actually we should probably build a nested object for the namespace.
                                // For now, let's just use the stem as the key if the configured key is empty/special?
                                // Or, if user provided a key "utils", we load all files into a "utils" map?

                                // Let's refine `collect_helpers`:
                                // Instead of flat insert, we should handle namespaces.
                                // But `load_helper_file` returns `Option<Value>`.
                                // Let's just insert as `stem` if glob is used and key is empty?
                                // If key is provided, we probably want to merge into that key?
                                // Complex to do with current `HelperDefs` structure.

                                // Compromise: If glob, we use the file stem as the key.
                                // We ignore `helper.key` for globs for now to avoid merging complexity in Rust
                                // without a robust deep-merge utility handy.
                                // Wait, Pytemplify says "variable: path".
                                // If path is glob, "variable" is the dictionary holding the results.
                                // We need deep merge or at least collecting into a map.

                                let namespace = helper.key.clone();
                                if !namespace.is_empty() {
                                    let entry = entries.entry(namespace).or_insert_with(|| {
                                        serde_json::Value::Object(serde_json::Map::new())
                                    });
                                    if let serde_json::Value::Object(map) = entry {
                                        let stem =
                                            path.file_stem().unwrap().to_string_lossy().to_string();
                                        map.insert(stem, val);
                                    }
                                } else {
                                    // No namespace, inject at top level with filename stem
                                    let stem =
                                        path.file_stem().unwrap().to_string_lossy().to_string();
                                    entries.insert(stem.clone(), val.clone());
                                    globals.insert(stem, val.clone());
                                }
                            }
                        }
                    }
                    Err(e) => warn!("Glob error: {}", e),
                }
            }
        } else {
            if let Some(val) = load_helper_file(helper, base)? {
                entries.insert(helper.key.clone(), val.clone());
                globals.insert(helper.key.clone(), val);
            }
        }
    }

    Ok(HelperDefs {
        globals,
        context_entries: entries,
    })
}

fn parse_helper_arg(raw: &str) -> Result<HelperConfig> {
    let parts: Vec<&str> = raw.splitn(2, '=').collect();
    if parts.len() != 2 {
        return Err(anyhow::anyhow!("--helper expects key=path"));
    }
    Ok(HelperConfig {
        key: parts[0].to_string(),
        path: parts[1].to_string(),
        format: "auto".to_string(),
    })
}

fn load_helper_file(helper: &HelperConfig, base: &Path) -> Result<Option<serde_json::Value>> {
    let helper_path = base.join(&helper.path);
    let format = detect_format(&helper.format, &helper_path);
    let content = match std::fs::read_to_string(&helper_path) {
        Ok(c) => c,
        Err(_) => {
            return Err(anyhow::anyhow!("Helper file not found: {:?}", helper_path));
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

fn handle_manual_sections(_cli: &Cli, action: ManualAction) -> Result<()> {
    match action {
        ManualAction::Backup { input, backup } => {
            let manager = ManualSectionManager::new(ManualSectionConfig::default());
            let mut backup_data = HashMap::new();

            if input.is_dir() {
                for entry in walkdir::WalkDir::new(&input) {
                    let entry = entry?;
                    if entry.file_type().is_file() {
                        let path = entry.path();
                        let content = std::fs::read_to_string(path).ok();
                        if let Some(c) = content {
                            let blocks = manager.extract_blocks(&c);
                            if !blocks.is_empty() {
                                let rel_path =
                                    path.strip_prefix(&input)?.to_string_lossy().to_string();
                                backup_data.insert(rel_path, blocks);
                            }
                        }
                    }
                }
            } else {
                let content = std::fs::read_to_string(&input)
                    .with_context(|| format!("Failed to read input: {:?}", input))?;
                let blocks = manager.extract_blocks(&content);
                if !blocks.is_empty() {
                    let filename = input.file_name().unwrap().to_string_lossy().to_string();
                    backup_data.insert(filename, blocks);
                }
            }

            let serialized = serde_json::to_string_pretty(&backup_data)?;
            std::fs::write(&backup, serialized)
                .with_context(|| format!("Failed to write backup: {:?}", backup))?;
            info!("Manual sections backed up to {:?}", backup);
        }
        ManualAction::Restore {
            input,
            backup,
            output,
        } => {
            let backup_str = std::fs::read_to_string(&backup)
                .with_context(|| format!("Failed to read backup: {:?}", backup))?;
            let backup_data: HashMap<String, HashMap<String, String>> =
                serde_json::from_str(&backup_str)
                    .or_else(|_| serde_yaml::from_str(&backup_str))
                    .with_context(|| format!("Failed to parse backup: {:?}", backup))?;

            let manager = ManualSectionManager::new(ManualSectionConfig::default());

            if input.is_dir() {
                for (rel_path, blocks) in backup_data {
                    let target_path = input.join(&rel_path);
                    if target_path.exists() {
                        let content = std::fs::read_to_string(&target_path)
                            .with_context(|| format!("Failed to read target: {:?}", target_path))?;
                        let restored = manager.restore_blocks(&content, &blocks);

                        let out_path = if let Some(ref out_dir) = output {
                            out_dir.join(&rel_path)
                        } else {
                            target_path
                        };

                        if let Some(parent) = out_path.parent() {
                            std::fs::create_dir_all(parent)?;
                        }

                        std::fs::write(&out_path, restored)
                            .with_context(|| format!("Failed to write output: {:?}", out_path))?;
                        info!("Restored sections for {:?}", out_path);
                    } else {
                        warn!("Target file for restore not found: {:?}", target_path);
                    }
                }
            } else {
                let filename = input.file_name().unwrap().to_string_lossy().to_string();
                let blocks = backup_data
                    .get(&filename)
                    .or_else(|| backup_data.values().next());

                if let Some(blocks) = blocks {
                    let content = std::fs::read_to_string(&input)
                        .with_context(|| format!("Failed to read input: {:?}", input))?;
                    let restored = manager.restore_blocks(&content, blocks);
                    let out_path = output.clone().unwrap_or(input.clone());
                    std::fs::write(&out_path, restored)
                        .with_context(|| format!("Failed to write output: {:?}", out_path))?;
                    info!("Manual sections restored into {:?}", out_path);
                } else {
                    warn!("No manual sections found in backup for {:?}", input);
                }
            }
        }
    }
    Ok(())
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
