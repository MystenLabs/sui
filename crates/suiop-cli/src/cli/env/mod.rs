// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use crate::run_cmd;
use anyhow::Result;
use clap::Parser;
use inquire::Select;
use std::io::Write;
use tracing::{debug, info};

/// Load an environment from pulumi
///
/// if no environment name is provided, the user will be prompted to select one from the list
#[derive(Parser, Debug)]
pub struct LoadEnvironmentArgs {
    /// the optional name of the environment to load
    environment_name: Option<String>,
}

pub fn load_environment_cmd(args: &LoadEnvironmentArgs) -> Result<()> {
    setup_pulumi_environment(&args.environment_name.clone().unwrap_or_else(|| {
        let output = run_cmd(vec!["pulumi", "env", "ls"], None).expect("Running pulumi env ls");
        let output_str = String::from_utf8_lossy(&output.stdout);
        let options: Vec<&str> = output_str.lines().collect();

        if options.is_empty() {
            panic!("No environments found. Make sure you are logged into the correct pulumi org.");
        }

        Select::new("Select an environment:", options)
            .prompt()
            .expect("Failed to select environment")
            .to_owned()
    }))
}

pub fn setup_pulumi_environment(environment_name: &str) -> Result<()> {
    let output = run_cmd(vec!["pulumi", "env", "open", environment_name], None)?;
    let output_str = String::from_utf8_lossy(&output.stdout);
    let output_json: serde_json::Value = serde_json::from_str(&output_str)?;
    let env_vars = &output_json["environmentVariables"];
    // Open a file to write environment variables
    let home_dir = std::env::var("HOME").expect("HOME environment variable not set");
    let suiop_dir = format!("{}/.suiop", home_dir);
    std::fs::create_dir_all(&suiop_dir).expect("Failed to create .suiop directory");
    let env_file_path = format!("{}/env_vars", suiop_dir);
    let mut env_file =
        std::fs::File::create(&env_file_path).expect("Failed to create env_vars file");

    if let serde_json::Value::Object(env_vars) = env_vars {
        for (key, value) in env_vars {
            if let Some(value_str) = value.as_str() {
                writeln!(env_file, "{}={}", key, value_str)?;
                info!("writing environment variable {}", key);
                debug!("={}", value_str);
            } else {
                info!(
                    "Failed to set environment variable: {}. Value is not a string.",
                    key
                );
            }
        }
    } else {
        info!("Environment variables are not in the expected format.");
        debug!("env: {:?}", output_json);
    }
    info!(
        "finished loading environment. use `source {}` to load them into your shell",
        env_file_path
    );
    Ok(())
}
