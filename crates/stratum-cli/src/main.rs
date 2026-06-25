use anyhow::Result;
use clap::{Parser, Subcommand};
use std::io::Read;
use stratum_core::content::ContentType;
use stratum_core::mode::Mode;
use stratum_core::pipeline::{CompressionContext, CompressionPipeline};
use stratum_core::store::InMemoryOffloadStore;
use stratum_hosts::build_rules;

#[derive(Parser)]
#[command(name = "stratum", version = env!("CARGO_PKG_VERSION"))]
struct Cli {
    #[command(subcommand)]
    cmd: Command,
}

#[derive(Subcommand)]
enum Command {
    /// Run the pipeline on stdin.
    Run,
    /// Show or set the active mode.
    Mode { value: Option<String> },
    /// Emit the ruleset for the active mode.
    Rules,
    /// Print version and exit.
    Version,
}

fn main() -> Result<()> {
    let cli = Cli::parse();
    match cli.cmd {
        Command::Run => {
            let mut input = String::new();
            std::io::stdin().read_to_string(&mut input)?;
            let pipeline = CompressionPipeline::new();
            let store = InMemoryOffloadStore::new();
            let output = pipeline.run(
                &input,
                ContentType::PlainText,
                &CompressionContext::default(),
                &store,
            );
            print!("{output}");
        }
        Command::Mode { value } => {
            let mode = match value {
                Some(v) => v.parse::<Mode>().map_err(|e| anyhow::anyhow!("{e}"))?,
                None => Mode::Full,
            };
            println!("{mode}");
        }
        Command::Rules => {
            println!("{}", build_rules(Mode::Full));
        }
        Command::Version => {
            println!("stratum {}", env!("CARGO_PKG_VERSION"));
        }
    }
    Ok(())
}
