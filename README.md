# templify
File generation helper using a data dictionary and Jinja2 template files.

## Input
- **Data dictionary**: The data used for rendering templates.
- **Templates**: The template files used for rendering. Supported formats:
  - Jinja2 templates (`*.j2`)
  - Injection templates (`*.inj`)

## Output
- Rendered or updated files based on the provided templates and data dictionary.


## Additional Features
- **Recursive Rendering**: Recursively render all `*.j2` files in the given template folder.
- **Dynamic File/Folder Names**: File or folder names can be rendered using the data dictionary.
- **Manual Sections**: Preserve specific sections in the output files that should not be overwritten when generating again. These sections are marked with `MANUAL SECTION START` and `MANUAL SECTION END`.
- **Injection Templates**: Use `*.inj` templates with regex patterns to inject content into specific parts of the output files.

## Usage

### Example Code
```rust
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
```

### Example Template (`template.j2`)
```jinja
Hello, {{ context.name }}!
This is a test template.
```

### Example Output
```
Hello, World!
This is a test template.
```

### Manual Sections
To preserve specific sections in the output files, use the following markers in your templates:
```cpp
// MANUAL SECTION START: init-custom-var
// This content will be preserved.
custom_var = 1;
// MANUAL SECTION END
```

### Injection Templates
To inject content into specific parts of the output files, use the following format in your templates:
```jinja
<!-- injection-pattern: test-pattern -->
^(?P<injection>.*)$
<!-- injection-string-start -->
Injected Content
<!-- injection-string-end -->
```