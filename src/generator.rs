use log::{error, info, warn};
use regex::Regex;
use serde::Serialize;
use std::{fs, path::Path};

use crate::engine::TemplateEngine;
use crate::manual_sections::ManualSectionManager;
use crate::formatting::FormatterManager;

/// The regex pattern for injection points.
const INJECTION_PATTERN: &str = r"<!-- injection-pattern: (?P<name>[a-zA-Z0-9_-]+) -->";
const INJECTION_STRING_START: &str = "<!-- injection-string-start -->";
const INJECTION_STRING_END: &str = "<!-- injection-string-end -->";

pub struct FileGenerator {
    engine: TemplateEngine,
    manual_section_manager: ManualSectionManager,
    formatter_manager: Option<FormatterManager>,
    dry_run: bool,
}

impl FileGenerator {
    pub fn new(
        engine: TemplateEngine,
        manual_section_manager: ManualSectionManager,
        dry_run: bool,
    ) -> Self {
        Self {
            engine,
            manual_section_manager,
            formatter_manager: None, // Default to None, use with_formatter to set
            dry_run,
        }
    }
    
    pub fn with_formatter(mut self, formatter_manager: FormatterManager) -> Self {
        self.formatter_manager = Some(formatter_manager);
        self
    }

    /// Ensures that the specified directory exists, creating it if necessary.
    fn ensure_dir_exists(path: &Path) -> Result<(), String> {
        if !path.exists() {
            fs::create_dir_all(path).map_err(|e| e.to_string())?;
        }
        Ok(())
    }

    /// Generates files from the specified template path to the output path.
    pub fn generate<T: Serialize>(
        &self,
        template_path: &Path,
        output_path: &Path,
        context: &T,
    ) -> Result<(), String> {
        self.generate_internal(template_path, output_path, context, true)
    }

    /// Internal method to generate files from the specified template path to the output path.
    fn generate_internal<T: Serialize>(
        &self,
        template_path: &Path,
        output_path: &Path,
        context: &T,
        root_path: bool,
    ) -> Result<(), String> {
        if !template_path.exists() {
            error!("Template file does not exist: {:?}", template_path);
            return Err("Template file does not exist".to_string());
        }

        if !self.dry_run {
            Self::ensure_dir_exists(output_path)?;
        }

        if template_path.is_file() {
            let filename = template_path.file_name().unwrap().to_str().unwrap();
            let filename = filename
                .strip_suffix(".j2")
                .or_else(|| filename.strip_suffix(".inj"))
                .unwrap_or(filename);
            let rendered_filename = self.engine.render_string(filename, context)?;
            let new_output_path = output_path.join(rendered_filename);
            self.generate_file(template_path, &new_output_path, context)?;
        } else {
            let folder_name = template_path.file_name().unwrap().to_str().unwrap();
            let rendered_folder_name = self.engine.render_string(folder_name, context)?;
            let new_output_path = if root_path {
                output_path.to_path_buf()
            } else {
                output_path.join(&rendered_folder_name)
            };
            for entry in fs::read_dir(template_path).map_err(|e| {
                error!("Failed to read directory: {:?}", template_path);
                e.to_string()
            })? {
                let entry = entry.map_err(|e| {
                    error!("Failed to read directory entry: {:?}", template_path);
                    e.to_string()
                })?;
                let path = entry.path();
                self.generate_internal(&path, &new_output_path, context, false)?;
            }
        }
        Ok(())
    }

    /// Generates a file from the specified template path to the output path.
    fn generate_file<T: Serialize>(
        &self,
        template_path: &Path,
        output_path: &Path,
        context: &T,
    ) -> Result<(), String> {
        if output_path.file_name().is_none() {
            error!("Output path must have a filename: {:?}", output_path);
            return Err("Output path must have a filename".to_string());
        }

        let prev_rendered_string = if output_path.exists() {
            fs::read_to_string(output_path)
                .map_err(|e| {
                    error!("Failed to read output file: {:?}", output_path);
                    e.to_string()
                })
                .ok()
        } else {
            None
        };

        if let Some(parent) = output_path.parent() {
            if !self.dry_run {
                Self::ensure_dir_exists(parent)?;
            }
        }

        if let Some(ext) = template_path.extension() {
            if ext == "j2" {
                let rendered_content = self.engine.render_file(template_path, context)?;
                
                // Validate manual sections
                self.manual_section_manager.validate_sections(
                    template_path.to_str().unwrap_or("template"), 
                    &rendered_content, 
                    prev_rendered_string.as_deref()
                )?;

                let mut final_content = if let Some(prev) = prev_rendered_string.as_deref() {
                    self.manual_section_manager.preserve_sections(&rendered_content, prev)
                } else {
                    rendered_content
                };
                
                // Format content
                if let Some(fmt) = &self.formatter_manager {
                    final_content = fmt.format_content(&final_content, output_path.to_str().unwrap_or(""));
                }

                if self.dry_run {
                    info!("[DRY RUN] Would write: {:?}", output_path);
                } else {
                    fs::write(output_path, final_content).map_err(|e| {
                        error!(
                            "Failed to write rendered content to file: {:?}",
                            output_path
                        );
                        e.to_string()
                    })?;
                    info!("{:?}", output_path);
                }
            } else if ext == "inj" && prev_rendered_string.is_some() {
                let injected_content =
                    self.inject_string(template_path, prev_rendered_string.as_deref(), context)?;
                
                if self.dry_run {
                    info!("[DRY RUN] Would inject: {:?}", output_path);
                } else {
                    fs::write(output_path, injected_content).map_err(|e| {
                        error!(
                            "Failed to write injected content to file: {:?}",
                            output_path
                        );
                        e.to_string()
                    })?;
                    info!("{:?}", output_path);
                }
            } else {
                if self.dry_run {
                    info!("[DRY RUN] Would copy: {:?}", output_path);
                } else {
                    fs::copy(template_path, output_path).map_err(|e| {
                        error!(
                            "Failed to copy file from {:?} to {:?}",
                            template_path, output_path
                        );
                        e.to_string()
                    })?;
                    info!("{:?}", output_path);
                }
            }
        } else {
            if self.dry_run {
                info!("[DRY RUN] Would copy: {:?}", output_path);
            } else {
                fs::copy(template_path, output_path).map_err(|e| {
                    error!(
                        "Failed to copy file from {:?} to {:?}",
                        template_path, output_path
                    );
                    e.to_string()
                })?;
                info!("{:?}", output_path);
            }
        }
        Ok(())
    }

    /// Injects a string into prev_rendered_string.
    fn inject_string<T: Serialize>(
        &self,
        template_path: &Path,
        prev_rendered_string: Option<&str>,
        context: &T,
    ) -> Result<String, String> {
        let template_str = fs::read_to_string(template_path).map_err(|e| {
            error!("Failed to read template file: {:?}", template_path);
            e.to_string()
        })?;
        let rendered_string = self.engine.render_string(&template_str, context)?;
        let re_pattern = Regex::new(INJECTION_PATTERN).unwrap();
        let mut modifications = Vec::new();

        for cap in re_pattern.captures_iter(&rendered_string) {
            let name = cap.name("name").unwrap().as_str();
            let section_body = &rendered_string[cap.get(0).unwrap().end()..];
            let pattern_text = section_body
                .split(INJECTION_STRING_START)
                .next()
                .unwrap()
                .trim();
            let re_injection = Regex::new(pattern_text)
                .map_err(|e| format!("Invalid regex pattern '{}': {}", pattern_text, e))?;
            if !pattern_text.contains("(?P<injection>") {
                return Err(format!(
                    "Invalid regex pattern '{}': no 'injection' named capture group",
                    pattern_text
                ));
            }
            let injection_string = section_body
                .split(INJECTION_STRING_START)
                .nth(1)
                .unwrap()
                .split(INJECTION_STRING_END)
                .next()
                .unwrap();
            let mut found = false;
            if let Some(prev_rendered_string) = prev_rendered_string {
                for m in re_injection.captures_iter(prev_rendered_string) {
                    found = true;
                    let injection_start = m.name("injection").unwrap().start();
                    let injection_end = m.name("injection").unwrap().end();
                    modifications.push((
                        injection_start,
                        injection_end,
                        injection_string.to_string(),
                    ));
                }
            }
            if !found {
                warn!("Failed to inject '{}':\\n{}", name, pattern_text);
            }
        }

        if let Some(prev_rendered_string) = prev_rendered_string {
            modifications.sort_by_key(|x| x.0);
            let mut modified_buffer = String::new();
            let mut last_pos = 0;
            for (injection_start, injection_end, injection_string) in modifications {
                modified_buffer.push_str(&prev_rendered_string[last_pos..injection_start]);
                modified_buffer.push_str(&injection_string);
                last_pos = injection_end;
            }
            modified_buffer.push_str(&prev_rendered_string[last_pos..]);
            return Ok(modified_buffer);
        }
        Ok(rendered_string)
    }
}
