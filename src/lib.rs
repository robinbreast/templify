// Export public modules
pub mod config;
pub mod engine;
pub mod generator;
pub mod iteration;
pub mod manual_sections;
pub mod filters;
pub mod formatting;

// Re-export commonly used types
pub use config::{ManualSectionConfig, TemplateConfig};
pub use engine::TemplateEngine;
pub use generator::FileGenerator;
pub use iteration::{IterationEvaluator, IterationPattern};
pub use manual_sections::ManualSectionManager;

// Legacy compatibility: RenderHelper facade
use serde::Serialize;
use std::collections::HashMap;
use std::path::Path;

/// Legacy RenderHelper for backward compatibility.
/// This is a facade that wraps the new architecture.
#[deprecated(since = "0.2.0", note = "Use TemplateEngine and FileGenerator directly")]
pub struct RenderHelper {
    generator: FileGenerator,
    context: serde_json::Value,
}

#[allow(deprecated)]
impl RenderHelper {
    /// Creates a new `RenderHelper` instance.
    pub fn new<T: Serialize>(dict_data: T, dict_name: Option<&str>) -> Result<Self, String> {
        let context = if let Some(name) = dict_name {
            let mut map = HashMap::new();
            map.insert(name.to_string(), serde_json::to_value(&dict_data).unwrap());
            serde_json::to_value(map).unwrap()
        } else {
            serde_json::to_value(&dict_data).unwrap()
        };

        let engine = TemplateEngine::new();
        let manual_section_manager = ManualSectionManager::new(ManualSectionConfig::default());
        let generator = FileGenerator::new(engine, manual_section_manager, false);

        Ok(Self { generator, context })
    }

    /// Generates files from the specified template path to the output path.
    pub fn generate(&self, template_path: &Path, output_path: &Path) -> Result<(), String> {
        self.generator.generate(template_path, output_path, &self.context)
    }
}
