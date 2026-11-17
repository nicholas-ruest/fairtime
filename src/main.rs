use clap::{Parser, ValueEnum};
use fairtime::{load_config, plan, render_text};
use std::{path::PathBuf, process::ExitCode};

#[derive(Debug, Parser)]
#[command(version, about)]
struct Cli {
    /// TOML file describing participants and preferred local hours.
    config: PathBuf,
    /// Output format.
    #[arg(long, value_enum, default_value_t = OutputFormat::Text)]
    format: OutputFormat,
}

#[derive(Debug, Clone, Copy, ValueEnum)]
enum OutputFormat {
    Text,
    Json,
}

fn main() -> ExitCode {
    match run() {
        Ok(output) => {
            print!("{output}");
            ExitCode::SUCCESS
        }
        Err(error) => {
            eprintln!("fairtime: {error}");
            ExitCode::FAILURE
        }
    }
}

fn run() -> Result<String, Box<dyn std::error::Error>> {
    let cli = Cli::parse();
    let config = load_config(&cli.config)?;
    let plan = plan(&config)?;
    match cli.format {
        OutputFormat::Text => Ok(render_text(&plan)),
        OutputFormat::Json => Ok(format!("{}\n", serde_json::to_string_pretty(&plan)?)),
    }
}
