use std::path::Path;
use templify::RenderHelper;
use env_logger;
use std::env;
use log::{info, error};

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

    // Create a RenderHelper instance
    let render_helper = match RenderHelper::new(person, Some("person")) {
        Ok(helper) => helper,
        Err(e) => {
            error!("Failed to create RenderHelper: {}", e);
            return;
        }
    };

    // Define paths for template and output files
    let template_path = Path::new("examples/templates/struct/template.j2");
    let output_folder = Path::new("output/struct");

    // Generate the output file from the template file
    match render_helper.generate(template_path, output_folder) {
        Ok(_) => info!("Files generated successfully in: {:?}", output_folder),
        Err(e) => error!("{}", e),
    }
}
