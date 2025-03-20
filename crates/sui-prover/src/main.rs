
use std::path::PathBuf;

use clap::*;
use colored::Colorize;
use prove::{BoogieConfig, GeneralConfig, BuildConfig, execute};
use tracing::debug;

mod prove;

bin_version::bin_version!();

#[derive(Parser)]
#[clap(
    name = env!("CARGO_BIN_NAME"),
    about = "A command-line tool for formal verification of Move code in Sui projects. When run in the root of a project, it executes all proofs automatically.",
    rename_all = "kebab-case",
    author,
    version = VERSION,
)]
struct Args {
    /// Path to a package which the command should be run with respect to.
    #[clap(long = "path", short = 'p', global = true)]
    pub package_path: Option<PathBuf>,

    /// General options
    #[clap(flatten)]
    pub general_config: GeneralConfig,

    /// Boggie options
    #[clap(flatten)]
    pub boogie_config: BoogieConfig,

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
        .with_env()
        .init();

    debug!("Sui-Prover CLI version: {VERSION}");

    let result = execute(args.package_path.as_deref(), args.general_config, args.build_config, args.boogie_config);

    match result {
        Ok(_) => (),
        Err(err) => {
            let err = format!("{:?}", err);
            println!("{}", err.bold().red());
            std::process::exit(1);
        }
    }
}
