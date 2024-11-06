use log::{error, info, warn};
use minijinja::{Environment, UndefinedBehavior};
use regex::Regex;
use serde::Serialize;
use std::{collections::HashMap, fs, path::Path};

/// The start marker for manual sections.
const MANUAL_SECTION_START: &str = "MANUAL SECTION START";

/// The end marker for manual sections.
const MANUAL_SECTION_END: &str = "MANUAL SECTION END";

/// The regex pattern for manual section IDs.
const MANUAL_SECTION_ID: &str = "[a-zA-Z0-9_-]+";

/// The regex pattern for injection points.
const INJECTION_PATTERN: &str = r"<!-- injection-pattern: (?P<name>[a-zA-Z0-9_-]+) -->";

/// The start marker for injection strings.
const INJECTION_STRING_START: &str = "<!-- injection-string-start -->";

/// The end marker for injection strings.
const INJECTION_STRING_END: &str = "<!-- injection-string-end -->";

/// A helper struct for rendering templates using the `minijinja` library.
pub struct RenderHelper {
    /// The `minijinja` environment used for rendering templates.
    env: Environment<'static>,

    /// The context data used in the templates.
    context: minijinja::value::Value,
}

impl RenderHelper {
    /// Creates a new `RenderHelper` instance.
    ///
    /// # Arguments
    ///
    /// * `dict_data` - The data to be used in the template context.
    /// * `dict_name` - An optional name for the context dictionary.
    ///
    /// # Returns
    ///
    /// A `Result` containing the `RenderHelper` instance or an error message.
    pub fn new<T: Serialize>(dict_data: T, dict_name: Option<&str>) -> Result<Self, String> {
        let mut env = Environment::new();
        let context = {
            let mut map = HashMap::new();
            map.insert(
                dict_name.unwrap_or("dict_data").to_string(),
                minijinja::value::Value::from_serialize(&dict_data),
            );
            minijinja::value::Value::from(map)
        };
        env.set_undefined_behavior(UndefinedBehavior::Strict);
        Ok(Self { env, context })
    }

    /// Ensures that the specified directory exists, creating it if necessary.
    ///
    /// # Arguments
    ///
    /// * `path` - The path to the directory.
    ///
    /// # Returns
    ///
    /// A `Result` indicating success or containing an error message.
    fn ensure_dir_exists(path: &Path) -> Result<(), String> {
        if !path.exists() {
            fs::create_dir_all(path).map_err(|e| e.to_string())?;
        }
        Ok(())
    }

    /// Generates files from the specified template path to the output path.
    ///
    /// # Arguments
    ///
    /// * `template_path` - The path to the template.
    /// * `output_path` - The path to the output directory.
    ///
    /// # Returns
    ///
    /// A `Result` indicating success or containing an error message.
    pub fn generate(&self, template_path: &Path, output_path: &Path) -> Result<(), String> {
        // first call with root_path = true
        self.generate_internal(template_path, output_path, true)
    }

    /// Internal method to generate files from the specified template path to the output path.
    ///
    /// # Arguments
    ///
    /// * `template_path` - The path to the template.
    /// * `output_path` - The path to the output directory.
    /// * `root_path` - A boolean indicating if this is the root path.
    ///
    /// # Returns
    ///
    /// A `Result` indicating success or containing an error message.
    fn generate_internal(
        &self,
        template_path: &Path,
        output_path: &Path,
        root_path: bool,
    ) -> Result<(), String> {
        if !template_path.exists() {
            error!("Template file does not exist: {:?}", template_path);
            return Err("Template file does not exist".to_string());
        }
        Self::ensure_dir_exists(output_path)?;
        if template_path.is_file() {
            let filename = template_path.file_name().unwrap().to_str().unwrap();
            let filename = filename
                .strip_suffix(".j2")
                .or_else(|| filename.strip_suffix(".inj"))
                .unwrap_or(filename);
            let rendered_filename = self.render_from_string(filename, None)?;
            let new_output_path = output_path.join(rendered_filename);
            self.generate_file(template_path, &new_output_path)?;
        } else {
            let folder_name = template_path.file_name().unwrap().to_str().unwrap();
            let rendered_folder_name = self.render_from_string(folder_name, None)?;
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
                // second call with root_path = false
                self.generate_internal(&path, &new_output_path, false)?;
            }
        }
        Ok(())
    }

    /// Renders a template from a string.
    ///
    /// # Arguments
    ///
    /// * `template_str` - The template string.
    /// * `prev_rendered_string` - An optional previously rendered string.
    ///
    /// # Returns
    ///
    /// A `Result` containing the rendered string or an error message.
    fn render_from_string(
        &self,
        template_str: &str,
        prev_rendered_string: Option<&str>,
    ) -> Result<String, String> {
        let template = self
            .env
            .template_from_str(template_str)
            .map_err(|e| e.to_string())?;
        let rendered = template.render(&self.context).map_err(|e| {
            if let Some(line) = e.line() {
                let error_line = template_str.lines().nth(line - 1).unwrap_or("");
                format!("{}\n{}", e, error_line)
            } else {
                format!("{}", e)
            }
        })?;
        if let Some(prev) = prev_rendered_string {
            return Ok(preserve_manual_sections(&rendered, prev));
        }
        Ok(rendered)
    }

    /// Renders a template from a file.
    ///
    /// # Arguments
    ///
    /// * `template_path` - The path to the template file.
    /// * `prev_rendered_string` - An optional previously rendered string.
    ///
    /// # Returns
    ///
    /// A `Result` containing the rendered string or an error message.
    fn render_from_file(
        &self,
        template_path: &Path,
        prev_rendered_string: Option<&str>,
    ) -> Result<String, String> {
        let template_str = fs::read_to_string(template_path).map_err(|e| {
            error!("Failed to read template file: {:?}", template_path);
            e.to_string()
        })?;
        self.render_from_string(&template_str, prev_rendered_string)
            .map_err(|e| format!("{:?}, error: {}", template_path, e))
    }

    /// Injects a string into prev_rendered_string.
    ///
    /// # Arguments
    ///
    /// * `template_path` - The path to the template file.
    /// * `prev_rendered_string` - An optional previously rendered string.
    ///
    /// # Returns
    ///
    /// A `Result` containing the injected string or an error message.
    fn inject_string(
        &self,
        template_path: &Path,
        prev_rendered_string: Option<&str>,
    ) -> Result<String, String> {
        let template_str = fs::read_to_string(template_path).map_err(|e| {
            error!("Failed to read template file: {:?}", template_path);
            e.to_string()
        })?;
        let rendered_string = self.render_from_string(&template_str, None)?;
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
                warn!("Failed to inject '{}':\n{}", name, pattern_text);
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

    /// Generates a file from the specified template path to the output path.
    ///
    /// # Arguments
    ///
    /// * `template_path` - The path to the template file.
    /// * `output_path` - The path to the output file.
    ///
    /// # Returns
    ///
    /// A `Result` indicating success or containing an error message.
    fn generate_file(&self, template_path: &Path, output_path: &Path) -> Result<(), String> {
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
            Self::ensure_dir_exists(parent)?;
        }
        if let Some(ext) = template_path.extension() {
            if ext == "j2" {
                let rendered_content =
                    self.render_from_file(template_path, prev_rendered_string.as_deref())?;
                fs::write(output_path, rendered_content).map_err(|e| {
                    error!(
                        "Failed to write rendered content to file: {:?}",
                        output_path
                    );
                    e.to_string()
                })?;
            } else if ext == "inj" && prev_rendered_string.is_some() {
                let injected_content =
                    self.inject_string(template_path, prev_rendered_string.as_deref())?;
                fs::write(output_path, injected_content).map_err(|e| {
                    error!(
                        "Failed to write injected content to file: {:?}",
                        output_path
                    );
                    e.to_string()
                })?;
            } else {
                fs::copy(template_path, output_path).map_err(|e| {
                    error!(
                        "Failed to copy file from {:?} to {:?}",
                        template_path, output_path
                    );
                    e.to_string()
                })?;
            }
        } else {
            fs::copy(template_path, output_path).map_err(|e| {
                error!(
                    "Failed to copy file from {:?} to {:?}",
                    template_path, output_path
                );
                e.to_string()
            })?;
        }
        info!("{:?}", output_path);
        Ok(())
    }
}

fn preserve_manual_sections(new_rendered: &str, prev_rendered: &str) -> String {
    let manual_section_pattern = format!(
        r"{}: ({})(?:\s|$)(?s)(.*?){}",
        MANUAL_SECTION_START, MANUAL_SECTION_ID, MANUAL_SECTION_END
    );
    let re = Regex::new(&manual_section_pattern).unwrap();
    let mut preserved = String::new();
    let mut last_end = 0;

    for cap in re.captures_iter(new_rendered) {
        let start = cap.get(0).unwrap().start();
        let end = cap.get(0).unwrap().end();
        let id = cap.get(1).unwrap().as_str();

        preserved.push_str(&new_rendered[last_end..start]);

        let prev_cap = re
            .captures_iter(prev_rendered)
            .find(|c| c.get(1).unwrap().as_str() == id);

        if let Some(prev_cap) = prev_cap {
            preserved.push_str(prev_cap.get(0).unwrap().as_str());
        } else {
            preserved.push_str(&new_rendered[start..end]);
        }

        last_end = end;
    }

    preserved.push_str(&new_rendered[last_end..]);
    preserved
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashMap;
    use std::fs::File;
    use std::io::Write;
    use tempfile::tempdir;

    #[test]
    fn test_generate() {
        let data = HashMap::from([("key", "value")]);
        let render_helper =
            RenderHelper::new(data, Some("context")).expect("Failed to create render helper");
        let template_content = r#"
Hello, {{ context.key }}!
This is a test template.
"#
        .trim();
        let dir = tempdir().unwrap();
        let template_path = dir.path().join("template.j2");
        let output_path = dir.path().join("output");

        // Write template content to a file
        let mut file = File::create(&template_path).unwrap();
        writeln!(file, "{}", template_content).unwrap();

        // Generate files
        render_helper
            .generate(&template_path, &output_path)
            .unwrap();

        // Verify output
        let output_file_path = output_path.join("template");
        let output_content = fs::read_to_string(output_file_path).unwrap();
        assert_eq!(
            output_content.trim(),
            "Hello, value!\nThis is a test template."
        );
    }

    #[test]
    fn test_generate_file() {
        let data = HashMap::from([("key", "value")]);
        let render_helper =
            RenderHelper::new(data, Some("context")).expect("Failed to create render helper");
        let template_content = r#"
Hello, {{ context.key }}!
This is a test template.
"#
        .trim();
        let dir = tempdir().unwrap();
        let template_path = dir.path().join("template.j2");
        let output_path = dir.path().join("output.txt");

        // Write template content to a file
        let mut file = File::create(&template_path).unwrap();
        writeln!(file, "{}", template_content).unwrap();

        // Generate file
        render_helper
            .generate_file(&template_path, &output_path)
            .unwrap();

        // Verify output
        let output_content = fs::read_to_string(output_path).unwrap();
        assert_eq!(
            output_content.trim(),
            "Hello, value!\nThis is a test template."
        );
    }

    #[test]
    fn test_render_from_string() {
        let data = HashMap::from([("key", "value".to_string())]);
        let render_helper =
            RenderHelper::new(data, Some("context")).expect("Failed to create render helper");
        let template_str = r#"
Hello, {{ context.key }}!
This is a test template.
"#
        .trim();
        let rendered = render_helper
            .render_from_string(template_str, None)
            .unwrap();
        assert_eq!(rendered.trim(), "Hello, value!\nThis is a test template.");
    }

    #[test]
    fn test_render_from_file() {
        let data = HashMap::from([("key", "value".to_string())]);
        let render_helper =
            RenderHelper::new(data, Some("context")).expect("Failed to create render helper");
        let template_content = r#"
Hello, {{ context.key }}!
This is a test template.
"#
        .trim();
        let dir = tempdir().unwrap();
        let template_path = dir.path().join("template.j2");

        // Write template content to a file
        let mut file = File::create(&template_path).unwrap();
        writeln!(file, "{}", template_content).unwrap();

        // Render from file
        let rendered = render_helper
            .render_from_file(&template_path, None)
            .unwrap();
        assert_eq!(rendered.trim(), "Hello, value!\nThis is a test template.");
    }

    #[test]
    fn test_inject_string() {
        let data = HashMap::from([("key", "value")]);
        let render_helper =
            RenderHelper::new(data, Some("context")).expect("Failed to create render helper");
        let template_content = r#"
<!-- injection-pattern: test-pattern -->
^(?P<injection>.*)$
<!-- injection-string-start -->
Injected Content
<!-- injection-string-end -->
"#
        .trim();
        let prev_content = "Some previous content";
        let dir = tempdir().unwrap();
        let template_path = dir.path().join("template.inj");

        // Write template content to a file
        let mut file = File::create(&template_path).unwrap();
        writeln!(file, "{}", template_content).unwrap();

        // Inject string
        let injected = render_helper
            .inject_string(&template_path, Some(prev_content))
            .unwrap();
        assert!(injected.contains("Injected Content"));
    }

    #[test]
    fn test_preserve_manual_sections() {
        let new_rendered = r#"
Start
MANUAL SECTION START: section1
New Content
MANUAL SECTION END
End
"#
        .trim();
        let prev_rendered = r#"
Start
MANUAL SECTION START: section1
Old Content
MANUAL SECTION END
End
"#
        .trim();
        let result = preserve_manual_sections(new_rendered, prev_rendered);
        assert!(result.contains("Old Content"));
    }

    #[test]
    fn test_generate_file_template_not_exist() {
        let data = HashMap::from([("key", "value")]);
        let render_helper =
            RenderHelper::new(data, Some("context")).expect("Failed to create render helper");
        let template_path = Path::new("non_existent_template.j2");
        let output_path = Path::new("output.txt");

        let result = render_helper.generate_file(template_path, output_path);
        assert!(result.is_err());
    }

    #[test]
    fn test_generate_file_invalid_output_path() {
        let data = HashMap::from([("key", "value")]);
        let render_helper =
            RenderHelper::new(data, Some("context")).expect("Failed to create render helper");
        let template_content = r#"
Hello, {{ context.key }}!
This is a test template.
"#
        .trim();
        let dir = tempdir().unwrap();
        let template_path = dir.path().join("template.j2");
        let output_path = dir.path().join("output/");

        // Write template content to a file
        let mut file = File::create(&template_path).unwrap();
        writeln!(file, "{}", template_content).unwrap();

        let result = render_helper.generate_file(&template_path, &output_path);
        assert!(result.is_err());
    }

    #[test]
    fn test_render_from_string_invalid_template() {
        let data = HashMap::from([("key", "value")]);
        let render_helper =
            RenderHelper::new(data, Some("context")).expect("Failed to create render helper");
        let template_str = r#"
Hello, {{ context.key }}!
This is a test template.
Here is an invalid key: {{ context.invalid_key }}.
"#
        .trim();

        let result = render_helper.render_from_string(template_str, None);
        assert!(result.is_err());
    }

    #[test]
    fn test_render_from_file_invalid_template() {
        let data = HashMap::from([("key", "value")]);
        let render_helper =
            RenderHelper::new(data, Some("context")).expect("Failed to create render helper");
        let template_content = r#"
Hello, {{ context.key }}!
This is a test template.
Here is an invalid key: {{ context.invalid_key }}.
"#
        .trim();
        let dir = tempdir().unwrap();
        let template_path = dir.path().join("template.j2");

        // Write invalid template content to a file
        let mut file = File::create(&template_path).unwrap();
        writeln!(file, "{}", template_content).unwrap();

        let result = render_helper.render_from_file(&template_path, None);
        assert!(result.is_err());
    }
}
