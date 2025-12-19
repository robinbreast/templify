use anyhow::{Context, Result};
use clap::{Parser, Subcommand};
use log::{error, info, warn};
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use templify::config::TemplateConfig;
use templify::iteration::IterationEvaluator;
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
}

fn main() -> Result<()> {
    env_logger::init();

    let cli = Cli::parse();

    match cli.command {
        Some(Commands::Init { path }) => {
            init_project(&path)?;
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

    let output_base = cli.output.unwrap_or_else(|| {
        config_path
            .parent()
            .unwrap_or(Path::new("."))
            .to_path_buf()
    });

    if cli.dry_run {
        info!("=== DRY RUN MODE ===");
    }

    for template_set in config.templates {
        if !template_set.enabled {
            continue;
        }

        // Filter check
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

        let engine = TemplateEngine::new();
        let manual_section_manager =
            ManualSectionManager::new(config.manual_sections.clone());
            
        // Initialize formatter
        let formatter_manager = templify::formatting::FormatterManager::new(
            config.format.clone(),
            manual_section_manager.clone(), // Clone needed because FileGenerator takes ownership? No, we need to pass a clone if we need it elsewhere but ManualSectionManager is cheap to clone usually
        );
            
        let generator = FileGenerator::new(engine, manual_section_manager, cli.dry_run)
            .with_formatter(formatter_manager);

        if let Some(iterate) = template_set.iterate {
            let info = IterationEvaluator::parse_simple(&iterate)
                .map_err(|e| anyhow::anyhow!("Failed to parse iteration: {}", e))?;
            
            let path = IterationEvaluator::evaluate_path(&info.expr);
            let items = data.pointer(&path);

            if let Some(serde_json::Value::Array(items)) = items {
                for item in items.iter() {
                    // TODO: Check condition if present
                    let mut context = HashMap::new();

                    // Add globals
                    if let Some(ref globals) = config.globals {
                        context.insert(
                            "globals".to_string(),
                            serde_json::to_value(globals).unwrap(),
                        );
                    }

                    // Add iteration variable
                    context.insert(info.var.clone(), item.clone());

                    // Add 'dd' (full data)
                    context.insert("dd".to_string(), data.clone());

                    // Flatten data if enabled
                    if config.flatten_data {
                        if let serde_json::Value::Object(map) = &data {
                            for (k, v) in map {
                                context.insert(k.clone(), v.clone());
                            }
                        }
                    }

                    generator
                        .generate(&template_folder, &set_output_path, &context)
                        .map_err(|e| anyhow::anyhow!(e))?;
                }
            } else {
                error!(
                    "Iteration expression '{}' did not resolve to an array",
                    info.expr
                );
            }
        } else {
            // Static generation
            let mut context = HashMap::new();
            
            // Add globals
            if let Some(ref globals) = config.globals {
                context.insert(
                    "globals".to_string(),
                    serde_json::to_value(globals).unwrap(),
                );
            }
            
            // Add 'dd' (full data)
            context.insert("dd".to_string(), data.clone());
            
            // Add extra data
            for extra in &config.extra_data {
                let extra_path = config_path.parent().unwrap_or(Path::new(".")).join(&extra.path);
                match std::fs::read_to_string(&extra_path) {
                    Ok(content) => {
                         let val: serde_json::Value = if extra.path.ends_with(".yaml") || extra.path.ends_with(".yml") {
                             serde_yaml::from_str(&content).unwrap_or(serde_json::Value::Null)
                         } else {
                             serde_json::from_str(&content).unwrap_or(serde_json::Value::Null)
                         };
                         
                         // Check valid
                         if val.is_null() {
                              warn!("Failed to parse extra data from {:?}", extra_path);
                              if extra.required {
                                  return Err(anyhow::anyhow!("Required extra data file failed to parse: {:?}", extra_path));
                              }
                         } else {
                              context.insert(extra.key.clone(), val);
                         }
                    },
                    Err(_) => {
                        if extra.required {
                            return Err(anyhow::anyhow!("Required extra data file not found: {:?}", extra_path));
                        } else {
                            warn!("Optional extra data file not found: {:?}", extra_path);
                        }
                    }
                }
            }

            // Flatten data if enabled
            if config.flatten_data {
                if let serde_json::Value::Object(map) = &data {
                    for (k, v) in map {
                        context.insert(k.clone(), v.clone());
                    }
                }
            }

            generator
                .generate(&template_folder, &set_output_path, &context)
                .map_err(|e| anyhow::anyhow!(e))?;
        }
    }

    if cli.dry_run {
        info!("=== DRY RUN COMPLETE ===");
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
