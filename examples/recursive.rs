use std::collections::HashMap;
use std::path::Path;
use templify::{TemplateEngine, FileGenerator, ManualSectionManager, ManualSectionConfig};
use env_logger;
use std::env;
use log::{info, error};

fn main() {
    // Initialize the logger with the desired logging level
    env::set_var("RUST_LOG", "info");
    env_logger::init();

    // Example data to be used in the template
    let mut data = HashMap::new();
    data.insert("project_name", "Templify");
    data.insert("author", "robinbreast");

    // Initialize components
    let engine = TemplateEngine::new();
    let manual_section_manager = ManualSectionManager::new(ManualSectionConfig::default());
    let generator = FileGenerator::new(engine, manual_section_manager, false);

    // Define paths for template and output files
    let template_path = Path::new("examples/templates/recursive");
    let output_folder = Path::new("output/recursive");

    // Generate the output file from the template file
    match generator.generate(template_path, output_folder, &data) {
        Ok(_) => info!("Files generated successfully in: {:?}", output_folder),
        Err(e) => error!("{}", e),
    }
}
