use std::collections::HashMap;
use std::path::Path;
use templify::{TemplateEngine, FileGenerator, ManualSectionManager, ManualSectionConfig};
use env_logger;
use std::env;
use log::{info, error};
use fs_extra::dir::{copy, CopyOptions, remove};

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
    let template_path = Path::new("examples/templates/inject");
    // Prepare output folder by copying examples/targets/inject to output/inject
    let output_path = Path::new("output/inject");
    if output_path.exists() {
        if let Err(e) = remove(output_path) {
            error!("Failed to remove existing output folder: {}", e);
            return;
        }
    }
    let mut options = CopyOptions::new();
    options.copy_inside = true; // Recursively copy contents
    if let Err(e) = copy("examples/targets/inject", "output/inject", &options) {
        error!("Failed to copy folder: {}", e);
        return;
    }

    // Generate the output file from the template file
    match generator.generate(template_path, output_path, &data) {
        Ok(_) => info!("Files generated successfully in: {:?}", output_path),
        Err(e) => error!("{}", e),
    }
}
