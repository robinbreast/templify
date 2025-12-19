use crate::config::{FormatConfig, FormatterConfig};
use crate::manual_sections::ManualSectionManager;
use log::{debug, error, warn};
use std::io::Write;
use std::process::{Command, Stdio};

pub struct FormatterManager {
    config: FormatConfig,
    manual_section_manager: ManualSectionManager,
}

impl FormatterManager {
    pub fn new(config: FormatConfig, manual_section_manager: ManualSectionManager) -> Self {
        Self {
            config,
            manual_section_manager,
        }
    }

    pub fn format_content(&self, content: &str, filename: &str) -> String {
        if !self.config.enabled {
            return content.to_string();
        }

        if self.should_ignore(filename) {
            debug!("Ignored file for formatting: {}", filename);
            return content.to_string();
        }

        if let Some(formatter_config) = self.get_formatter_for_file(filename) {
            let preserve = self.config.defaults.preserve_manual_sections;
            
            // Extract manual sections if needed
            let blocks = if preserve {
                 Some(self.manual_section_manager.extract_blocks(content))
            } else {
                None
            };
            
            // Format
            let formatted = self.run_formatter(content, formatter_config, filename);
            
            // Restore manual sections
            if let Some(blocks) = blocks {
                 self.manual_section_manager.restore_blocks(&formatted, &blocks)
            } else {
                 formatted
            }
        } else {
            content.to_string()
        }
    }

    fn should_ignore(&self, filename: &str) -> bool {
        for pattern in &self.config.defaults.ignore_patterns {
            // Simple check
            if filename.contains(pattern) || filename.ends_with(pattern.trim_start_matches('*')) {
                return true;
            }
        }
        false
    }

    fn get_formatter_for_file(&self, filename: &str) -> Option<&FormatterConfig> {
        // pattern matching logic
        // formatters keys are patterns, e.g. "*.rs" or "rust" (not ideal design in original config but let's assume keys are patterns)
        for (pattern, config) in &self.config.formatters {
            if !config.enabled {
                continue;
            }
            if self.matches_pattern(filename, pattern) {
                return Some(config);
            }
        }
        None
    }

    fn matches_pattern(&self, filename: &str, pattern: &str) -> bool {
        if pattern.starts_with("*.") {
            filename.ends_with(&pattern[1..])
        } else {
            filename == pattern || filename.ends_with(pattern)
        }
    }

    fn run_formatter(&self, content: &str, config: &FormatterConfig, filename: &str) -> String {
        if config.formatter_type != "command" {
            warn!("Unsupported formatter type: {}", config.formatter_type);
            return content.to_string();
        }

        let cmd_str = match &config.command {
            Some(c) => c,
            None => return content.to_string(),
        };

        let mut cmd = Command::new(cmd_str);
        if let Some(args) = &config.args {
            cmd.args(args);
        }
        
        // Pass content via stdin
        cmd.stdin(Stdio::piped()).stdout(Stdio::piped()).stderr(Stdio::piped());

        debug!("Running formatter {} on {}", cmd_str, filename);

        let mut child = match cmd.spawn() {
            Ok(c) => c,
            Err(e) => {
                error!("Failed to spawn formatter: {}", e);
                return content.to_string();
            }
        };

        if let Some(mut stdin) = child.stdin.take() {
            if let Err(e) = stdin.write_all(content.as_bytes()) {
                error!("Failed to write to formatter stdin: {}", e);
                return content.to_string();
            }
        }

        let output = match child.wait_with_output() {
            Ok(o) => o,
            Err(e) => {
                error!("Failed to wait for formatter: {}", e);
                return content.to_string();
            }
        };

        if output.status.success() {
            String::from_utf8_lossy(&output.stdout).to_string()
        } else {
            let stderr = String::from_utf8_lossy(&output.stderr);
            error!("Formatter failed: {}", stderr);
            // Fallback to original content
            content.to_string()
        }
    }
}
