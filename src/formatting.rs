use crate::config::{FormatConfig, FormatterConfig};
use crate::manual_sections::ManualSectionManager;
use log::{debug, error};
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
                self.manual_section_manager
                    .restore_blocks(&formatted, &blocks)
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
        if let Some(regex_pat) = pattern.strip_prefix("regex:") {
            if let Ok(re) = regex::Regex::new(regex_pat) {
                return re.is_match(filename);
            }
        }
        if pattern.starts_with("*.") {
            return filename.ends_with(&pattern[1..]);
        }
        if pattern.contains('*') {
            let parts: Vec<&str> = pattern.split('*').collect();
            if parts.len() == 2 {
                return filename.starts_with(parts[0]) && filename.ends_with(parts[1]);
            }
        }
        filename == pattern || filename.ends_with(pattern)
    }

    fn run_formatter(&self, content: &str, config: &FormatterConfig, filename: &str) -> String {
        if config.formatter_type == "clang-format" || config.formatter_type == "cpp_format" {
            return self.run_clang_formatter(content, config, filename);
        }

        let cmd_str = match config.formatter_type.as_str() {
            "command" => config.command.as_deref().unwrap_or(""),
            other => config.command.as_deref().unwrap_or(other),
        };

        if cmd_str.is_empty() {
            return content.to_string();
        }

        let mut cmd = Command::new(cmd_str);
        if let Some(args) = &config.args {
            cmd.args(args);
        } else {
            match config.formatter_type.as_str() {
                "black" | "autopep8" | "yapf" => {
                    for (k, v) in &config.options {
                        if let Some(s) = v.as_str() {
                            cmd.arg(format!("--{}", k));
                            cmd.arg(s);
                        } else if let Some(b) = v.as_bool() {
                            if b {
                                cmd.arg(format!("--{}", k));
                            }
                        } else if let Some(n) = v.as_i64() {
                            cmd.arg(format!("--{}", k));
                            cmd.arg(n.to_string());
                        }
                    }
                    cmd.arg("-");
                }
                "prettier" => {
                    for (k, v) in &config.options {
                        if let Some(s) = v.as_str() {
                            cmd.arg(format!("--{}", k));
                            cmd.arg(s);
                        } else if let Some(b) = v.as_bool() {
                            if b {
                                cmd.arg(format!("--{}", k));
                            } else {
                                cmd.arg(format!("--no-{}", k));
                            }
                        } else if let Some(n) = v.as_i64() {
                            cmd.arg(format!("--{}", k));
                            cmd.arg(n.to_string());
                        }
                    }
                    cmd.args(["--stdin-filepath", filename]);
                }
                _ => {
                    for (k, v) in &config.options {
                        if let Some(s) = v.as_str() {
                            cmd.arg(format!("--{}", k));
                            cmd.arg(s);
                        } else if let Some(b) = v.as_bool() {
                            if b {
                                cmd.arg(format!("--{}", k));
                            }
                        } else if let Some(n) = v.as_i64() {
                            cmd.arg(format!("--{}", k));
                            cmd.arg(n.to_string());
                        }
                    }
                }
            }
        }

        cmd.stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped());

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
            content.to_string()
        }
    }

    fn run_clang_formatter(
        &self,
        content: &str,
        config: &FormatterConfig,
        filename: &str,
    ) -> String {
        let style_str = self.build_clang_style(&config.options);

        let cmd_str = config.command.as_deref().unwrap_or("clang-format");

        let mut cmd = Command::new(cmd_str);

        // Add style arg
        cmd.arg("-style");
        cmd.arg(&style_str);

        // Add args from config if needed (e.g. -assume-filename)
        if let Some(args) = &config.args {
            cmd.args(args);
        } else {
            // Default args if none provided
            cmd.arg("-assume-filename").arg(filename);
        }

        // Pass content via stdin
        cmd.stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped());

        debug!(
            "Running clang-format on {} with style: {}",
            filename, style_str
        );

        let mut child = match cmd.spawn() {
            Ok(c) => c,
            Err(e) => {
                error!("Failed to spawn clang-format: {}", e);
                return content.to_string();
            }
        };

        if let Some(mut stdin) = child.stdin.take() {
            if let Err(e) = stdin.write_all(content.as_bytes()) {
                error!("Failed to write to clang-format stdin: {}", e);
                return content.to_string();
            }
        }

        let output = match child.wait_with_output() {
            Ok(o) => o,
            Err(e) => {
                error!("Failed to wait for clang-format: {}", e);
                return content.to_string();
            }
        };

        if output.status.success() {
            String::from_utf8_lossy(&output.stdout).to_string()
        } else {
            let stderr = String::from_utf8_lossy(&output.stderr);
            error!("Clang-format failed: {}", stderr);
            content.to_string()
        }
    }

    fn build_clang_style(
        &self,
        options: &std::collections::HashMap<String, serde_json::Value>,
    ) -> String {
        // If "style" key exists and is a string, use it directly (e.g. style: "Google")
        if let Some(serde_json::Value::String(s)) = options.get("style") {
            return format!("{{BasedOnStyle: {}}}", s);
        }

        // Format options dict
        let mut parts = Vec::new();
        for (k, v) in options {
            if k == "style" {
                continue;
            }

            let val_str = match v {
                serde_json::Value::Bool(b) => b.to_string(),
                serde_json::Value::Number(n) => n.to_string(),
                serde_json::Value::String(s) => s.clone(),
                _ => v.to_string(),
            };
            parts.push(format!("{}: {}", k, val_str));
        }

        if parts.is_empty() {
            return "{BasedOnStyle: Google}".to_string();
        }

        format!("{{{}}}", parts.join(", "))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::{FormatConfig, FormatDefaults, FormatterConfig, ManualSectionConfig};
    use std::collections::HashMap;

    fn new_manager(config: FormatConfig) -> FormatterManager {
        let manual_section_manager = ManualSectionManager::new(ManualSectionConfig::default());
        FormatterManager::new(config, manual_section_manager)
    }

    #[test]
    fn test_format_content_disabled() {
        let manager = new_manager(FormatConfig {
            enabled: false,
            ..FormatConfig::default()
        });
        let result = manager.format_content("hello", "file.txt");
        assert_eq!(result, "hello");
    }

    #[test]
    fn test_format_content_ignored_pattern() {
        let mut formatters = HashMap::new();
        formatters.insert(
            "*.rs".to_string(),
            FormatterConfig {
                formatter_type: "command".to_string(),
                command: Some("cat".to_string()),
                args: None,
                options: HashMap::new(),
                enabled: true,
            },
        );

        let manager = new_manager(FormatConfig {
            enabled: true,
            formatters,
            defaults: FormatDefaults {
                ignore_patterns: vec!["*.rs".to_string()],
                preserve_manual_sections: true,
            },
        });

        let result = manager.format_content("hello", "main.rs");
        assert_eq!(result, "hello");
    }

    #[test]
    fn test_format_content_runs_command() {
        let mut formatters = HashMap::new();
        formatters.insert(
            "*.txt".to_string(),
            FormatterConfig {
                formatter_type: "command".to_string(),
                command: Some("cat".to_string()),
                args: None,
                options: HashMap::new(),
                enabled: true,
            },
        );

        let manager = new_manager(FormatConfig {
            enabled: true,
            formatters,
            defaults: FormatDefaults::default(),
        });

        let result = manager.format_content("hello", "file.txt");
        assert_eq!(result, "hello");
    }

    #[test]
    fn test_matches_pattern_regex_and_wildcards() {
        let manager = new_manager(FormatConfig::default());
        assert!(manager.matches_pattern("foo/main.rs", "*.rs"));
        assert!(manager.matches_pattern("foo/main.rs", "regex:^foo/.*\\.rs$"));
        assert!(!manager.matches_pattern("foo/main.txt", "regex:^foo/.*\\.rs$"));
        assert!(!manager.matches_pattern("foo/main.rs", "regex:["));
    }

    #[test]
    fn test_should_ignore_patterns() {
        let manager = new_manager(FormatConfig {
            enabled: true,
            formatters: HashMap::new(),
            defaults: FormatDefaults {
                ignore_patterns: vec!["min.js".to_string(), "*.lock".to_string()],
                preserve_manual_sections: true,
            },
        });

        assert!(manager.should_ignore("vendor.min.js"));
        assert!(manager.should_ignore("Cargo.lock"));
        assert!(!manager.should_ignore("main.rs"));
    }
}
