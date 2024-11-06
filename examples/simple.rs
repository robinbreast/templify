use std::collections::HashMap;
use std::path::Path;
use templify::RenderHelper;
use env_logger;
use std::env;
use log::{info, error};

fn main() {
    // Initialize the logger with the desired logging level
    env::set_var("RUST_LOG", "info");
    env_logger::init();

    // Example data to be used in the template
    let mut data = HashMap::new();
    data.insert("name", "World");

    // Create a RenderHelper instance
    let render_helper = match RenderHelper::new(data, Some("context")) {
        Ok(helper) => helper,
        Err(e) => {
            error!("Failed to create RenderHelper: {}", e);
            return;
        }
    };

    // Define paths for template and output files
    let template_path = Path::new("examples/templates/simple/template.j2");
    let output_folder = Path::new("output/simple");

    // Generate the output file from the template file
    match render_helper.generate(template_path, output_folder) {
        Ok(_) => info!("Files generated successfully in: {:?}", output_folder),
        Err(e) => error!("{}", e),
    }
}