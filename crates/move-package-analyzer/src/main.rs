// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use clap::*;
use move_package_analyzer::{
    errors::PackageAnalyzerError, load_config, load_from_dir::load_from_directory,
    model::loader::build_environment, passes::pass_manager::run, query_indexer::query_packages,
};
use std::{env, path::PathBuf, time::Instant};
use tracing::info;

#[derive(Parser, Clone, Debug)]
#[clap(
    name = "Sui Package Analyzer",
    about = "Package loader and analyzer.\n\
    Load a set of packages either from a directory or from the DB and runs a set of passes on them.\n\
    Passes are defined in a yaml config file (passes.yaml).\n\
    Use the `--db-url` option to load packages from the DB. Use the `--packages-dir` option to load packages from a directory.\n\
    When loading from a DB one must have a url with proper privileges to get the package table.\n\
    `sui-tool dump-packages` can be used to dump packages from a DB to a directory and then run this tool over them.",
    rename_all = "kebab-case"
)]
#[clap(group(ArgGroup::new("input").required(true).args(&["db_url", "packages_dir"])))]
struct Args {
    /// Connection information for the Indexer's Postgres DB.
    #[clap(long, short)]
    db_url: Option<String>,

    /// Path to a directory containing packages as dumped by `sui-tool dump-packages`
    #[clap(long, short)]
    packages_dir: Option<PathBuf>,

    /// Run in verbose mode
    #[clap(long, short)]
    verbose: bool,

    /// Path to a yaml config containing the passes to run
    #[clap(long, short)]
    config_path: Option<PathBuf>,
}

fn main() -> Result<(), PackageAnalyzerError> {
    let _guard = telemetry_subscribers::TelemetryConfig::new()
        .with_env()
        .init();

    let args = Args::parse();
    info!("Parsed args: {:#?}", args);

    // load passes
    let config_path = if let Some(path) = args.config_path {
        path.join("passes.yaml")
    } else {
        env::current_dir()
            .map_err(|e| PackageAnalyzerError::BadConfig(format!("Cannot get current dir: {}", e)))?
            .join("passes.yaml")
    };
    let passes_config = load_config(&config_path)?;
    info!("Passes config: {:#?}", passes_config);

    // load packages
    let read_time_start = Instant::now();
    let packages = if let Some(db_url) = args.db_url {
        query_packages(db_url.as_str())?
    } else {
        load_from_directory(args.packages_dir.expect("packages_dir arg must exist"))?
    };
    let read_time_end = Instant::now();
    info!(
        "Read {} packages in {}ms",
        packages.len(),
        read_time_end.duration_since(read_time_start).as_millis(),
    );

    // build environemnt
    let load_time_start = Instant::now();
    let global_env = build_environment(packages);
    let load_time_end = Instant::now();
    info!(
        "Loaded {} packages in {}ms",
        global_env.packages.len(),
        load_time_end.duration_since(load_time_start).as_millis(),
    );

    // run passes
    let passes_time_start = Instant::now();
    run(&passes_config, &global_env);
    let passes_time_end = Instant::now();
    info!(
        "Run passes in {}ms",
        passes_time_end
            .duration_since(passes_time_start)
            .as_millis(),
    );

    Ok(())
}
