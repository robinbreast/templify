use minijinja::{Environment, UndefinedBehavior};
use serde::Serialize;

/// TemplateEngine wraps minijinja::Environment and provides a clean API for rendering templates.
pub struct TemplateEngine {
    env: Environment<'static>,
}

impl TemplateEngine {
    /// Creates a new TemplateEngine with default configuration.
    pub fn new() -> Self {
        let mut env = Environment::new();
        env.set_undefined_behavior(UndefinedBehavior::Strict);
        
        // Register custom filters
        env.add_filter("camelcase", crate::filters::filter_camelcase);
        env.add_filter("pascalcase", crate::filters::filter_pascalcase);
        env.add_filter("snakecase", crate::filters::filter_snakecase);
        env.add_filter("kebabcase", crate::filters::filter_kebabcase);
        env.add_filter("screamingsnakecase", crate::filters::filter_screamingsnakecase);
        env.add_filter("uuid_generate", crate::filters::filter_uuid_generate);
        
        // Register utility functions
        env.add_function("uuid_generate", crate::filters::filter_uuid_generate);

        Self { env }
    }

    /// Registers a global variable in the template environment.
    pub fn add_global<T: Serialize>(&mut self, name: String, value: T) {
        self.env.add_global(name, minijinja::value::Value::from_serialize(&value));
    }


    /// Renders a template string with the given context.
    pub fn render_string<T: Serialize>(&self, template_str: &str, context: &T) -> Result<String, String> {
        let template = self
            .env
            .template_from_str(template_str)
            .map_err(|e| e.to_string())?;
        
        let rendered = template.render(context).map_err(|e| {
            if let Some(line) = e.line() {
                let error_line = template_str.lines().nth(line - 1).unwrap_or("");
                format!("{}\\n{}", e, error_line)
            } else {
                format!("{}", e)
            }
        })?;
        
        Ok(rendered)
    }

    /// Renders a template from a file with the given context.
    pub fn render_file<T: Serialize>(&self, template_path: &std::path::Path, context: &T) -> Result<String, String> {
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
        let result = engine.render_string("Hello, {{ name }}!", &context).unwrap();
        assert_eq!(result, "Hello, World!");
    }

    #[test]
    fn test_render_string_with_globals() {
        let mut engine = TemplateEngine::new();
        engine.add_global("version".to_string(), "1.0.0");
        
        let context = HashMap::from([("name", "Test")]);
        let result = engine.render_string("{{ name }} v{{ version }}", &context).unwrap();
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
