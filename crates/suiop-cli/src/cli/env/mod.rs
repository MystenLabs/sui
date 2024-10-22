use crate::run_cmd;
use anyhow::Result;
use clap::Parser;
use inquire::Select;
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
    if let serde_json::Value::Object(env_vars) = env_vars {
        for (key, value) in env_vars {
            if let Some(value_str) = value.as_str() {
                std::env::set_var(&key, value_str);
                info!("setting environment variable {}", key);
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
    info!("finished loading environment");
    Ok(())
}
