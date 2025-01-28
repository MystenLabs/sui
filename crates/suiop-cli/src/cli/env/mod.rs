// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use crate::{command::CommandOptions, run_cmd};
use anyhow::Result;
use clap::Parser;
use inquire::Select;
use query_shell::get_shell_name;

/// Load an environment from pulumi
///
/// if no environment name is provided, the user will be prompted to select one from the list
#[derive(Parser, Debug)]
pub struct LoadEnvironmentArgs {
    /// the optional name of the environment to load
    environment_name: Option<String>,
}

pub fn load_environment(args: &LoadEnvironmentArgs) -> Result<()> {
    // list envs from pulumi using `pulumi env ls`
    let env = args.environment_name.clone().unwrap_or_else(|| {
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
    });

    // get the user's shell
    let shell = get_shell_name()?;
    // use `pulumi env run <env_name> -i <shell>` to load the environment into the shell
    let opts = CommandOptions::new(true, false);
    run_cmd(vec!["pulumi", "env", "run", &env, "-i", &shell], Some(opts))?;
    Ok(())
}
