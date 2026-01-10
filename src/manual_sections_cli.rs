use anyhow::{Context, Result};
use clap::{Parser, Subcommand};
use log::{info, warn};
use regex::Regex;
use std::collections::HashMap;
use std::path::{Path, PathBuf};

use crate::config::TemplateConfig;
use crate::{ManualSectionConfig, ManualSectionManager};

#[derive(Parser)]
#[command(author, version, about = "Manage manual section backups", long_about = None)]
pub struct ManualSectionsCli {
    #[arg(short, long)]
    pub config: Option<PathBuf>,
    #[command(subcommand)]
    pub action: ManualAction,
}

#[derive(Subcommand, Clone)]
pub enum ManualAction {
    Backup {
        #[arg(long)]
        input: PathBuf,
        #[arg(long, alias = "output")]
        backup: PathBuf,
    },
    Restore {
        #[arg(long)]
        input: PathBuf,
        #[arg(long)]
        backup: PathBuf,
        #[arg(long)]
        output: Option<PathBuf>,
        #[arg(long)]
        preview: bool,
        #[arg(long = "section-map")]
        section_map: Vec<String>,
    },
    View {
        #[arg(long)]
        backup: PathBuf,
        #[arg(long)]
        file: Option<String>,
        #[arg(long)]
        section: Option<String>,
    },
    Report {
        #[arg(long)]
        backup: PathBuf,
        #[arg(long)]
        output: Option<PathBuf>,
    },
}

pub fn run_manual_sections(config: Option<PathBuf>, action: ManualAction) -> Result<()> {
    let manual_config = manual_section_config(config)?;

    match action {
        ManualAction::Backup { input, backup } => {
            let manager = ManualSectionManager::new(manual_config.clone());
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
            preview,
            section_map,
        } => {
            let backup_data = load_backup_data(&backup)?;
            let section_map = parse_section_map(section_map)?;
            let manager = ManualSectionManager::new(manual_config.clone());

            if input.is_dir() {
                for (rel_path, blocks) in backup_data {
                    let target_path = input.join(&rel_path);
                    if target_path.exists() {
                        let content = std::fs::read_to_string(&target_path)
                            .with_context(|| format!("Failed to read target: {:?}", target_path))?;
                        let mapped_blocks =
                            apply_section_map(&blocks, &section_map, &manual_config);
                        let restored = manager.restore_blocks(&content, &mapped_blocks);

                        let out_path = if let Some(ref out_dir) = output {
                            out_dir.join(&rel_path)
                        } else {
                            target_path
                        };

                        if preview {
                            info!("[PREVIEW] Would restore sections into {:?}", out_path);
                            continue;
                        }

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
                    let mapped_blocks = apply_section_map(blocks, &section_map, &manual_config);
                    let restored = manager.restore_blocks(&content, &mapped_blocks);
                    let out_path = output.clone().unwrap_or(input.clone());

                    if preview {
                        info!("[PREVIEW] Would restore sections into {:?}", out_path);
                    } else {
                        std::fs::write(&out_path, restored)
                            .with_context(|| format!("Failed to write output: {:?}", out_path))?;
                        info!("Manual sections restored into {:?}", out_path);
                    }
                } else {
                    warn!("No manual sections found in backup for {:?}", input);
                }
            }
        }
        ManualAction::View {
            backup,
            file,
            section,
        } => {
            let backup_data = load_backup_data(&backup)?;
            view_backup_sections(&backup_data, file.as_deref(), section.as_deref());
        }
        ManualAction::Report { backup, output } => {
            let backup_data = load_backup_data(&backup)?;
            let report = render_backup_report(&backup_data);

            if let Some(output) = output {
                std::fs::write(&output, report)
                    .with_context(|| format!("Failed to write report: {:?}", output))?;
                info!("Manual sections report written to {:?}", output);
            } else {
                println!("{}", report);
            }
        }
    }

    Ok(())
}

fn manual_section_config(config_path: Option<PathBuf>) -> Result<ManualSectionConfig> {
    if let Some(path) = config_path {
        let config = TemplateConfig::load(&path).context("Failed to load config")?;
        Ok(config.manual_sections.clone())
    } else {
        Ok(ManualSectionConfig::default())
    }
}

fn load_backup_data(backup: &Path) -> Result<HashMap<String, HashMap<String, String>>> {
    let backup_str = std::fs::read_to_string(backup)
        .with_context(|| format!("Failed to read backup: {:?}", backup))?;
    serde_json::from_str(&backup_str)
        .or_else(|_| serde_yaml::from_str(&backup_str))
        .with_context(|| format!("Failed to parse backup: {:?}", backup))
}

fn parse_section_map(items: Vec<String>) -> Result<HashMap<String, String>> {
    let mut mapping = HashMap::new();
    for item in items {
        let parts: Vec<&str> = item.splitn(2, ':').collect();
        if parts.len() != 2 || parts[0].trim().is_empty() || parts[1].trim().is_empty() {
            return Err(anyhow::anyhow!(
                "Invalid section map '{}', expected old:new",
                item
            ));
        }
        mapping.insert(parts[0].trim().to_string(), parts[1].trim().to_string());
    }
    Ok(mapping)
}

fn apply_section_map(
    blocks: &HashMap<String, String>,
    section_map: &HashMap<String, String>,
    config: &ManualSectionConfig,
) -> HashMap<String, String> {
    let mut mapped = HashMap::new();
    for (id, block) in blocks {
        if let Some(new_id) = section_map.get(id) {
            let updated_block = replace_section_id(block, id, new_id, config);
            mapped.insert(new_id.clone(), updated_block);
        } else {
            mapped.insert(id.clone(), block.clone());
        }
    }
    mapped
}

fn replace_section_id(
    block: &str,
    old_id: &str,
    new_id: &str,
    config: &ManualSectionConfig,
) -> String {
    let pattern = format!(
        r"({}:\s*){}(\b)",
        regex::escape(&config.start_marker),
        regex::escape(old_id)
    );
    let re = Regex::new(&pattern).unwrap();
    re.replace(block, format!("${{1}}{}", new_id)).to_string()
}

fn view_backup_sections(
    backup_data: &HashMap<String, HashMap<String, String>>,
    file: Option<&str>,
    section: Option<&str>,
) {
    let mut lines = Vec::new();

    let files: Vec<(&String, &HashMap<String, String>)> = if let Some(file) = file {
        match backup_data.get_key_value(file) {
            Some(pair) => vec![pair],
            None => {
                warn!("No manual sections found for file: {}", file);
                return;
            }
        }
    } else {
        backup_data.iter().collect()
    };

    for (file_name, blocks) in files {
        lines.push(format!("# {}", file_name));
        if let Some(section) = section {
            if let Some(block) = blocks.get(section) {
                lines.push(block.to_string());
            } else {
                lines.push(format!("(section '{}' not found)", section));
            }
            continue;
        }

        if blocks.is_empty() {
            lines.push("(no sections)".to_string());
            continue;
        }

        for (section_id, block) in blocks {
            lines.push(format!("## {}", section_id));
            lines.push(block.to_string());
        }
    }

    println!("{}", lines.join("\n"));
}

fn render_backup_report(backup_data: &HashMap<String, HashMap<String, String>>) -> String {
    let mut lines = Vec::new();
    let total_files = backup_data.len();
    let total_sections: usize = backup_data.values().map(|b| b.len()).sum();

    lines.push("# Manual Sections Report".to_string());
    lines.push(format!("- Files: {}", total_files));
    lines.push(format!("- Sections: {}", total_sections));
    lines.push(String::new());

    let mut file_names: Vec<&String> = backup_data.keys().collect();
    file_names.sort();

    for file in file_names {
        if let Some(blocks) = backup_data.get(file) {
            lines.push(format!("## {}", file));
            lines.push(format!("- Sections: {}", blocks.len()));
            let mut section_names: Vec<&String> = blocks.keys().collect();
            section_names.sort();
            for name in section_names {
                lines.push(format!("  - {}", name));
            }
            lines.push(String::new());
        }
    }

    lines.join("\n")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_section_map() {
        let map = parse_section_map(vec!["old:new".to_string()]).unwrap();
        assert_eq!(map.get("old"), Some(&"new".to_string()));
        assert!(parse_section_map(vec!["bad".to_string()]).is_err());
    }

    #[test]
    fn test_apply_section_map() {
        let mut blocks = HashMap::new();
        blocks.insert(
            "old".to_string(),
            "MANUAL SECTION START: old\ncontent\nMANUAL SECTION END".to_string(),
        );
        let mut map = HashMap::new();
        map.insert("old".to_string(), "new".to_string());

        let config = ManualSectionConfig::default();
        let mapped = apply_section_map(&blocks, &map, &config);

        assert!(mapped.get("old").is_none());
        assert!(mapped
            .get("new")
            .unwrap()
            .contains("MANUAL SECTION START: new"));
    }
}
