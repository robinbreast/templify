use anyhow::Result;
use clap::Parser;
use templify::manual_sections_cli::{run_manual_sections, ManualSectionsCli};

fn main() -> Result<()> {
    env_logger::init();
    let cli = ManualSectionsCli::parse();
    run_manual_sections(cli.config, cli.action)
}
