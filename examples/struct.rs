use std::path::Path;
use templify::{TemplateEngine, FileGenerator, ManualSectionManager, ManualSectionConfig};
use env_logger;
use std::env;
use log::{info, error};
use std::collections::HashMap;

#[derive(serde::Serialize)]
struct Person {
    name: String,
}

#[allow(dead_code)]
impl Person {
    fn new(name: &str) -> Person {
        Person {
            name: name.to_string(),
        }
    }
    fn description(&self) -> String {
        format!("Hello, my name is {}", self.name)
    }
}

fn main() {
    // Initialize the logger with the desired logging level
    env::set_var("RUST_LOG", "info");
    env_logger::init();

    // Create a Person instance
    let person = Person::new("John Doe");

    // Wrap in "person" key
    let mut context = HashMap::new();
    context.insert("person".to_string(), person);

    // Initialize components
    let engine = TemplateEngine::new();
    let manual_section_manager = ManualSectionManager::new(ManualSectionConfig::default());
    let generator = FileGenerator::new(engine, manual_section_manager, false);

    // Define paths for template and output files
    let template_path = Path::new("examples/templates/struct/template.j2");
    let output_folder = Path::new("output/struct");

    // Generate the output file from the template file
    match generator.generate(template_path, output_folder, &context) {
        Ok(_) => info!("Files generated successfully in: {:?}", output_folder),
        Err(e) => error!("{}", e),
    }
}
