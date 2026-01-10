# templify

A powerful, Rust-based file generation engine using Jinja2 templates. Inspired by `pytemplify` and `scaffolder`.

`templify` allows you to scaffold projects, generate configuration files, and manage code generation key features like **manual section preservation**, **code formatting**, and **custom filters**.

## Features

- **Jinja2 Templates**: Full support for Jinja2 syntax (via `minijinja`).
- **Data-Driven**: Render templates using JSON data and configuration files.
- **Recursive Generation**: Process entire directory trees of templates.
- **Manual Section Preservation**: Define sections in your templates that persist across re-generations (perfect for mixing generated code with custom logic).
- **Validation**:
    - **Duplicate ID Detection**: Prevents errors from duplicate manual section IDs.
    - **Structure/Nesting Checks**: Ensures manual sections are correctly nested and closed.
    - **Loss Prevention**: Warns if a manual section from an existing file is missing in the new generation.
- **Code Formatting**: Automatically run external formatters (e.g., `rustfmt`, `black`, `prettier`) on generated files while *protecting* your manual sections.
- **Content Injection**: Inject code into specific points of existing files using regex patterns.
- **Custom Filters**: Includes a suite of string manipulation (camelCase, snake_case, etc.) and utility filters (UUID generation).
- **CLI Tool (`yagen`)**: A robust command-line interface for managing generation tasks.

## Installation

```bash
cargo install templify
# or build from source
cargo build --release
```

## CLI Usage (`yagen`)

`templify` comes with a CLI tool called `yagen` (Yet Another GENerator).

### Initialize a Project

Create a new project structure with example configuration:

```bash
yagen init my-new-project
```

### Generate Files

Run the generator using a config file and a data file:

```bash
yagen -c config.yaml -d data.json
```

**Options:**
- `-c, --config <FILE>`: Path to YAML config file.
- `-d, --data <FILE>`: Path to JSON data file.
- `-o, --output <DIR>`: Override output directory.
- `--dry-run`: Simulate generation without writing files.
- `--include <PATTERN>`: Only process templates matching pattern.
- `--exclude <PATTERN>`: Exclude templates matching pattern.

## Configuration (`config.yaml`)

```yaml
globals:
  version: "1.0.0"
  project: "MyProject"

# Load additional data files into the context
extra_data:
  - key: "env"
    path: "env.json"
    required: false

# Configure Manual Section markers
manual_sections:
  start_marker: "MANUAL SECTION START"
  end_marker: "MANUAL SECTION END"

# Configure Code Formatting
format:
  enabled: true
  defaults:
    preserve_manual_sections: true
    ignore_patterns: ["*.min.js"]
  formatters:
    "*.rs":
      type: "command"
      command: "rustfmt"
      enabled: true
    "*.py":
      type: "command"
      command: "black"
      args: ["-"] # read from stdin
      enabled: true
    "*.cpp":
      type: "clang-format" # Specialized type for clang-format
      # command: "clang-format" # Default
      options:
        BasedOnStyle: Google
        IndentWidth: 4
      enabled: true

templates:
  - name: "Core"
    folder: "templates/core"
    output: "output/src"
    enabled: true
  - name: "Models"
    folder: "templates/models"
    output: "output/models"
    iterate: "model in models" # Iterate over 'models' list in data.json
    enabled: true
```

## Template Features

### Filters

`templify` provides many built-in filters:

**String Filters:**
- `camelcase`: `foo_bar` -> `fooBar`
- `pascalcase`: `foo_bar` -> `FooBar`
- `snakecase`: `FooBar` -> `foo_bar`
- `kebabcase`: `FooBar` -> `foo-bar`
- `screamingsnakecase`: `fooBar` -> `FOO_BAR`

**Utility Filters:**
- `uuid_generate`: Generates a UUID.
    - `{{ uuid_generate() }}`: Random v4 UUID.
    - `{{ "my-seed" | uuid_generate }}`: Deterministic v5 UUID based on seed.

### Manual Sections

Preserve content between generations.

```rust
// MANUAL SECTION START: my-custom-logic
let x = "This code will be kept even if the template changes!";
// MANUAL SECTION END
```

**Validation Rules:**
- IDs must be unique within a file.
- Sections cannot be nested.
- If a section exists in the target file, it *must* exist in the new template output, or generation will fail to prevent data loss.

### Injections

Inject content into existing files using regex patterns (useful for adding routes, imports, etc.).

File extension: `.inj`

```jinja
<!-- injection-pattern: my_injection -->
// match regex to find injection point
^// INJECT HERE$
<!-- injection-string-start -->
Injected content: {{ context.value }}
<!-- injection-string-end -->
```

## Library Usage

You can use `templify` as a library in your Rust projects.

```rust
use std::path::Path;
use templify::{TemplateEngine, FileGenerator, ManualSectionManager, ManualSectionConfig};

fn main() -> Result<(), Box<dyn std::error::Error>> {
    // 1. Setup Engine
    let engine = TemplateEngine::new();

    // 2. Setup Manual Section Manager
    let ms_config = ManualSectionConfig::default(); // or customize markers
    let ms_manager = ManualSectionManager::new(ms_config);

    // 3. Setup Generator
    let dry_run = false;
    let generator = FileGenerator::new(engine, ms_manager, dry_run);

    // 4. Generate
    let template_path = Path::new("templates/my_template.j2");
    let output_path = Path::new("output/result.txt");
    let context = serde_json::json!({ "name": "World" });

    generator.generate(template_path, output_path, &context)?;

    Ok(())
}
```

## License

MIT