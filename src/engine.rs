use minijinja::syntax::SyntaxConfig;
use minijinja::{AutoEscape, Environment, UndefinedBehavior};
use serde::Serialize;

/// TemplateEngine wraps minijinja::Environment and provides a clean API for rendering templates.
pub struct TemplateEngine {
    env: Environment<'static>,
    newline_sequence: String,
}

impl TemplateEngine {
    /// Creates a new TemplateEngine with default configuration.
    pub fn new() -> Self {
        let mut env = Environment::new();
        env.set_undefined_behavior(UndefinedBehavior::Strict);
        Self::build(env, "\n".to_string())
    }

    pub fn with_env(config: crate::config::JinjaEnvConfig) -> Self {
        let mut env = Environment::new();
        env.set_undefined_behavior(UndefinedBehavior::Strict);
        env.set_trim_blocks(config.trim_blocks);
        env.set_lstrip_blocks(config.lstrip_blocks);
        env.set_keep_trailing_newline(config.keep_trailing_newline);

        if config.autoescape {
            env.set_auto_escape_callback(|_| AutoEscape::Html);
        }

        if config.line_statement_prefix.is_some() || config.line_comment_prefix.is_some() {
            let mut builder = SyntaxConfig::builder();
            if let Some(prefix) = config.line_statement_prefix {
                builder.line_statement_prefix(prefix);
            }
            if let Some(prefix) = config.line_comment_prefix {
                builder.line_comment_prefix(prefix);
            }
            if let Ok(syntax) = builder.build() {
                env.set_syntax(syntax);
            }
        }

        let newline_seq = config.newline_sequence.unwrap_or_else(|| "\n".to_string());
        Self::build(env, newline_seq)
    }

    fn build(mut env: Environment<'static>, newline_sequence: String) -> Self {
        env.add_filter("camelcase", crate::filters::filter_camelcase);
        env.add_filter("pascalcase", crate::filters::filter_pascalcase);
        env.add_filter("snakecase", crate::filters::filter_snakecase);
        env.add_filter("kebabcase", crate::filters::filter_kebabcase);
        env.add_filter(
            "screamingsnakecase",
            crate::filters::filter_screamingsnakecase,
        );
        env.add_filter("slugify", crate::filters::filter_slugify);
        env.add_filter("uuid_generate", crate::filters::filter_uuid_generate);
        env.add_filter("regex_replace", crate::filters::filter_regex_replace);
        env.add_filter("ternary", crate::filters::filter_ternary);
        env.add_filter("coalesce", crate::filters::filter_coalesce);
        env.add_filter("hash_md5", crate::filters::filter_hash_md5);
        env.add_filter("flatten", crate::filters::filter_flatten);
        env.add_filter("unique", crate::filters::filter_unique);
        env.add_filter("compact", crate::filters::filter_compact);
        env.add_filter("pluck", crate::filters::filter_pluck);
        env.add_filter("where", crate::filters::filter_where);
        env.add_filter("sort_by", crate::filters::filter_sort_by);
        env.add_filter("group_by", crate::filters::filter_group_by);
        env.add_filter("format_json", crate::filters::filter_format_json);
        env.add_filter("format_yaml", crate::filters::filter_format_yaml);
        env.add_filter("format_number", crate::filters::filter_format_number);
        env.add_filter("format_bytes", crate::filters::filter_format_bytes);
        env.add_filter("format_date", crate::filters::filter_format_date);
        env.add_filter("tojson", crate::filters::filter_tojson);

        // Register utility functions
        env.add_function("uuid_generate", crate::filters::filter_uuid_generate);

        Self {
            env,
            newline_sequence,
        }
    }

    /// Registers a global variable in the template environment.
    pub fn add_global<T: Serialize>(&mut self, name: String, value: T) {
        self.env
            .add_global(name, minijinja::value::Value::from_serialize(&value));
    }

    /// Renders a template string with the given context.
    pub fn render_string<T: Serialize>(
        &self,
        template_str: &str,
        context: &T,
    ) -> Result<String, String> {
        let template = self
            .env
            .template_from_str(template_str)
            .map_err(|e| e.to_string())?;

        let rendered = template.render(context).map_err(|e| {
            if let Some(line) = e.line() {
                let error_line = template_str.lines().nth(line - 1).unwrap_or("");
                format!("{}\n{}", e, error_line)
            } else {
                format!("{}", e)
            }
        })?;

        let final_output = if self.newline_sequence != "\n" {
            rendered.replace('\n', &self.newline_sequence)
        } else {
            rendered
        };

        Ok(final_output)
    }

    /// Renders a template from a file with the given context.
    pub fn render_file<T: Serialize>(
        &self,
        template_path: &std::path::Path,
        context: &T,
    ) -> Result<String, String> {
        let template_str = std::fs::read_to_string(template_path)
            .map_err(|e| format!("Failed to read template file {:?}: {}", template_path, e))?;

        self.render_string(&template_str, context)
            .map_err(|e| format!("{:?}, error: {}", template_path, e))
    }
}

impl Default for TemplateEngine {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashMap;

    #[test]
    fn test_render_string() {
        let engine = TemplateEngine::new();
        let context = HashMap::from([("name", "World")]);
        let result = engine
            .render_string("Hello, {{ name }}!", &context)
            .unwrap();
        assert_eq!(result, "Hello, World!");
    }

    #[test]
    fn test_render_string_with_globals() {
        let mut engine = TemplateEngine::new();
        engine.add_global("version".to_string(), "1.0.0");

        let context = HashMap::from([("name", "Test")]);
        let result = engine
            .render_string("{{ name }} v{{ version }}", &context)
            .unwrap();
        assert_eq!(result, "Test v1.0.0");
    }

    #[test]
    fn test_render_string_undefined_variable() {
        let engine = TemplateEngine::new();
        let context: HashMap<String, String> = HashMap::new();
        let result = engine.render_string("Hello, {{ name }}!", &context);
        assert!(result.is_err());
    }
}
