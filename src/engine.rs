use minijinja::syntax::SyntaxConfig;
use minijinja::{AutoEscape, Environment, UndefinedBehavior};
use serde::Serialize;

/// TemplateEngine wraps minijinja::Environment and provides a clean API for rendering templates.
pub struct TemplateEngine {
    env: Environment<'static>,
    newline_sequence: String,
}

fn format_render_error(err: minijinja::Error, template_str: &str) -> String {
    if let Some(line) = err.line() {
        let mut message = format!("{} (line {})", err, line);
        let error_line = template_str.lines().nth(line - 1).unwrap_or("");
        message.push('\n');
        message.push_str(error_line);
        return message;
    }

    err.to_string()
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
        env.add_filter("normalize", crate::filters::filter_normalize);
        env.add_filter("indent_custom", crate::filters::filter_indent_custom);
        env.add_filter("remove_prefix", crate::filters::filter_remove_prefix);
        env.add_filter("remove_suffix", crate::filters::filter_remove_suffix);
        env.add_filter("wrap_text", crate::filters::filter_wrap_text);
        env.add_filter("truncate_custom", crate::filters::filter_truncate_custom);
        env.add_filter("regex_search", crate::filters::filter_regex_search);
        env.add_filter("regex_findall", crate::filters::filter_regex_findall);
        env.add_filter("quote_string", crate::filters::filter_quote_string);
        env.add_filter("uppercase", crate::filters::filter_uppercase);
        env.add_filter("lowercase", crate::filters::filter_lowercase);
        env.add_filter("titlecase", crate::filters::filter_titlecase);
        env.add_filter("default", crate::filters::filter_default);
        env.add_filter("default_if_none", crate::filters::filter_default_if_none);
        env.add_filter("contains", crate::filters::filter_contains);
        env.add_filter("trim", crate::filters::filter_trim);
        env.add_filter("trim_start", crate::filters::filter_trim_start);
        env.add_filter("trim_end", crate::filters::filter_trim_end);
        env.add_filter("startswith", crate::filters::filter_startswith);
        env.add_filter("endswith", crate::filters::filter_endswith);
        env.add_filter("replace", crate::filters::filter_replace);
        env.add_filter("split", crate::filters::filter_split);
        env.add_filter("join", crate::filters::filter_join);
        env.add_filter("pad_start", crate::filters::filter_pad_start);
        env.add_filter("pad_end", crate::filters::filter_pad_end);
        env.add_filter("pad_left", crate::filters::filter_pad_left);
        env.add_filter("pad_right", crate::filters::filter_pad_right);
        env.add_filter("capitalize", crate::filters::filter_capitalize);
        env.add_filter("remove", crate::filters::filter_remove);
        env.add_filter("repeat", crate::filters::filter_repeat);
        env.add_filter("reverse", crate::filters::filter_reverse);
        env.add_filter("truncate", crate::filters::filter_truncate);
        env.add_filter("slice", crate::filters::filter_slice);
        env.add_filter("length", crate::filters::filter_length);
        env.add_filter("first", crate::filters::filter_first);
        env.add_filter("last", crate::filters::filter_last);
        env.add_filter("sum", crate::filters::filter_sum);
        env.add_filter("min", crate::filters::filter_min);
        env.add_filter("max", crate::filters::filter_max);
        env.add_filter("round", crate::filters::filter_round);
        env.add_filter("avg", crate::filters::filter_avg);
        env.add_filter("median", crate::filters::filter_median);
        env.add_filter("unique_by", crate::filters::filter_unique_by);
        env.add_filter("dict_merge", crate::filters::filter_dict_merge);
        env.add_filter("merge_dicts", crate::filters::filter_merge_dicts);
        env.add_filter("dict_keys", crate::filters::filter_dict_keys);
        env.add_filter("dict_values", crate::filters::filter_dict_values);
        env.add_filter("dict_items", crate::filters::filter_dict_items);
        env.add_filter("zip_lists", crate::filters::filter_zip_lists);
        env.add_filter("index_of", crate::filters::filter_index_of);
        env.add_filter("intersection", crate::filters::filter_intersection);
        env.add_filter("difference", crate::filters::filter_difference);
        env.add_filter("union", crate::filters::filter_union);
        env.add_filter("chunk", crate::filters::filter_chunk);
        env.add_filter("uuid_generate", crate::filters::filter_uuid_generate);
        env.add_filter("regex_replace", crate::filters::filter_regex_replace);
        env.add_filter("ternary", crate::filters::filter_ternary);
        env.add_filter("coalesce", crate::filters::filter_coalesce);
        env.add_filter("type_name", crate::filters::filter_type_name);
        env.add_filter("is_list", crate::filters::filter_is_list);
        env.add_filter("is_dict", crate::filters::filter_is_dict);
        env.add_filter("is_string", crate::filters::filter_is_string);
        env.add_filter("is_number", crate::filters::filter_is_number);
        env.add_filter("is_even", crate::filters::filter_is_even);
        env.add_filter("is_odd", crate::filters::filter_is_odd);
        env.add_filter("hash_md5", crate::filters::filter_hash_md5);
        env.add_filter("hash_sha256", crate::filters::filter_hash_sha256);
        env.add_filter("b64encode", crate::filters::filter_b64encode);
        env.add_filter("b64decode", crate::filters::filter_b64decode);
        env.add_filter("random_string", crate::filters::filter_random_string);
        env.add_filter("random_int", crate::filters::filter_random_int);
        env.add_filter("abs_value", crate::filters::filter_abs_value);
        env.add_filter("clamp", crate::filters::filter_clamp);
        env.add_filter("bool_to_string", crate::filters::filter_bool_to_string);
        env.add_filter("file_extension", crate::filters::filter_file_extension);
        env.add_filter("file_basename", crate::filters::filter_file_basename);
        env.add_filter("file_dirname", crate::filters::filter_file_dirname);
        env.add_filter("safe_divide", crate::filters::filter_safe_divide);
        env.add_filter("map_value", crate::filters::filter_map_value);
        env.add_filter("get_attr", crate::filters::filter_get_attr);
        env.add_filter("get_item", crate::filters::filter_get_item);
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
        env.add_filter(
            "format_percentage",
            crate::filters::filter_format_percentage,
        );
        env.add_filter("format_date", crate::filters::filter_format_date);
        env.add_filter("format_currency", crate::filters::filter_format_currency);
        env.add_filter("format_ordinal", crate::filters::filter_format_ordinal);
        env.add_filter("format_phone", crate::filters::filter_format_phone);
        env.add_filter(
            "format_xml_escape",
            crate::filters::filter_format_xml_escape,
        );
        env.add_filter(
            "format_sql_escape",
            crate::filters::filter_format_sql_escape,
        );
        env.add_filter("tojson", crate::filters::filter_tojson);

        // Register utility functions
        env.add_function("uuid_generate", crate::filters::filter_uuid_generate);
        env.add_function("random_string", crate::filters::filter_random_string);
        env.add_function("random_int", crate::filters::filter_random_int);

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

        let rendered = template
            .render(context)
            .map_err(|e| format_render_error(e, template_str))?;

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
        let err = result.unwrap_err();
        assert!(err.contains("line 1"));
        assert!(err.contains("Hello, {{ name }}!"));
    }
}
