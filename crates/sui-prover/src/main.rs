use std::path::PathBuf;

use clap::*;
use colored::Colorize;
use prove::{GeneralConfig, BuildConfig, execute};
use tracing::debug;

mod prove;
mod llm_explain;
mod prompts;
mod generator;
mod generator_options;
mod boogie_backend;

bin_version::bin_version!();

#[derive(Parser)]
#[clap(
    name = env!("CARGO_BIN_NAME"),
    about = "Command-line tool for formal verification of Move code within Sui projects. When executed from the project's root directory, it attempts to prove all specifications annotated with #[spec(prove)]",
    rename_all = "kebab-case",
    author,
    version = VERSION,
)]
struct Args {
    /// Path to package directory with a Move.toml inside
    #[clap(long = "path", short = 'p', global = true)]
    pub package_path: Option<PathBuf>,

    /// Boggie options
    #[clap(long = "boogie-config", short = 'b', global = true)]
    pub boogie_config: Option<String>,

    /// General options
    #[clap(flatten)]
    pub general_config: GeneralConfig,

    /// Package build options
    #[clap(flatten)]
    pub build_config: BuildConfig,
}

#[tokio::main]
async fn main() {
    #[cfg(windows)]
    colored::control::set_virtual_terminal(true).unwrap();

    let bin_name = env!("CARGO_BIN_NAME");
    let args = Args::parse();

    let _guard = telemetry_subscribers::TelemetryConfig::new()
        .with_log_file(&format!("{bin_name}.log"))
        .with_log_level("debug")
        .with_env()
        .init();

    debug!("Sui-Prover CLI version: {VERSION}");

    let result = execute(args.package_path.as_deref(), args.general_config, args.build_config, args.boogie_config).await;

    match result {
        Ok(_) => (),
        Err(err) => {
            let err = format!("{:?}", err);
            println!("{}", err.bold().red());
            std::process::exit(1);
        }
    }
}
