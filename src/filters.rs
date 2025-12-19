use heck::{ToKebabCase, ToLowerCamelCase, ToPascalCase, ToShoutySnakeCase, ToSnakeCase};
use uuid::Uuid;

// Export individual filter functions
pub use self::camelcase as filter_camelcase;
pub use self::pascalcase as filter_pascalcase;
pub use self::snakecase as filter_snakecase;
pub use self::kebabcase as filter_kebabcase;
pub use self::screamingsnakecase as filter_screamingsnakecase;
pub use self::uuid_generate as filter_uuid_generate;

/* 
   Note: We assume these match minijinja's Filter signature.
*/

pub fn camelcase(s: String) -> String {
    s.to_lower_camel_case()
}

pub fn pascalcase(s: String) -> String {
    s.to_pascal_case()
}

pub fn snakecase(s: String) -> String {
    s.to_snake_case()
}

pub fn kebabcase(s: String) -> String {
    s.to_kebab_case()
}

pub fn screamingsnakecase(s: String) -> String {
    s.to_shouty_snake_case()
}

pub fn uuid_generate(val: Option<String>) -> String {
    // If value is none/empty, generate random UUID (v4)
    // If value is string, generate deterministic UUID (v5)
    
    // Uuid::NAMESPACE_DNS is available
    const NAMESPACE_DNS: Uuid = Uuid::NAMESPACE_DNS;
    const TEMPLIFY_NS_NAME: &str = "com.github.pytemplify";
    let templify_ns = Uuid::new_v5(&NAMESPACE_DNS, TEMPLIFY_NS_NAME.as_bytes());

    match val {
        Some(s) if !s.is_empty() => Uuid::new_v5(&templify_ns, s.as_bytes()).to_string(),
        _ => Uuid::new_v4().to_string(),
    }
}
