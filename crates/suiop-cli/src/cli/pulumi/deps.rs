// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use crate::{command::CommandOptions, run_cmd};
use anyhow::Result;
use serde_yaml::Value;
use std::fs;
use std::path::{Path, PathBuf};
use tracing::{debug, info};

fn update_dependencies(path: &Path, runtime: &str) -> Result<()> {
    info!(
        "Updating dependencies for {} project at {}",
        runtime,
        path.display()
    );

    let mut cmd_opts = CommandOptions::new(false, false);
    cmd_opts.current_dir = Some(path.to_path_buf());
    let output = match runtime {
        "go" => run_cmd(vec!["go", "get", "-u"], Some(cmd_opts.clone()))
            .and_then(|_o| run_cmd(vec!["go", "mod", "tidy"], Some(cmd_opts))),
        "python" => {
            if !path.join("pyproject.toml").exists() {
                run_cmd(vec!["pulumi", "install"], Some(cmd_opts))
            } else {
                run_cmd(vec!["poetry", "update"], Some(cmd_opts))
            }
        }
        "typescript" => run_cmd(vec!["pnpm", "update"], Some(cmd_opts)),
        _ => unreachable!(),
    }?;
    debug!(
        "Command output: {:?}",
        String::from_utf8_lossy(&output.stdout)
    );
    if !output.stderr.is_empty() {
        debug!(
            "Command stderr: {:?}",
            String::from_utf8_lossy(&output.stderr)
        );
    }

    Ok(())
}

fn process_directory(
    dir_path: &Path,
    runtime_filter: &Option<String>,
) -> Result<Vec<(PathBuf, anyhow::Error)>> {
    let mut errors = Vec::new();

    let pulumi_yaml = dir_path.join("Pulumi.yaml");

    if pulumi_yaml.exists() {
        let update_result = (|| -> Result<()> {
            let contents = fs::read_to_string(&pulumi_yaml)?;
            let yaml: Value = serde_yaml::from_str(&contents)
                .map_err(|e| std::io::Error::new(std::io::ErrorKind::InvalidData, e))?;

            let runtime = yaml["runtime"]
                .as_str()
                .or_else(|| yaml["runtime"]["name"].as_str())
                .ok_or_else(|| {
                    std::io::Error::new(
                        std::io::ErrorKind::InvalidData,
                        "No runtime field found in Pulumi.yaml",
                    )
                })?;

            let runtime = runtime.to_lowercase();
            if !["typescript", "go", "python"].contains(&runtime.as_str()) {
                return Ok(());
            }

            if runtime_filter.as_ref().map_or(true, |f| f == &runtime) {
                info!("Updating dependencies for {}", runtime);
                update_dependencies(dir_path, &runtime)?;
            }
            Ok(())
        })();

        if let Err(e) = update_result {
            errors.push((dir_path.to_path_buf(), e));
        }
    }

    // Recurse into subdirectories
    for entry in fs::read_dir(dir_path)? {
        let entry = entry?;
        let path = entry.path();
        let file_name = path.file_name().and_then(|n| n.to_str()).unwrap_or("");
        if path.is_dir()
            && !file_name.starts_with('.')
            && !file_name.contains("common")
            && !file_name.contains("node_modules")
        {
            info!("Processing subdirectory: {}", path.display());
            match process_directory(&path, runtime_filter) {
                Ok(mut sub_errors) => errors.append(&mut sub_errors),
                Err(e) => errors.push((path, e)),
            }
        }
    }
    Ok(errors)
}

pub fn update_deps_cmd(filepath: PathBuf, runtime: Option<String>) -> Result<()> {
    if !filepath.exists() || !filepath.is_dir() {
        return Err(anyhow::anyhow!(
            "Specified path does not exist or is not a directory",
        ));
    }

    let errors = process_directory(&filepath, &runtime)?;
    if !errors.is_empty() {
        let error_messages = errors
            .into_iter()
            .map(|(path, error)| format!("- {}: {}", path.display(), error))
            .collect::<Vec<_>>()
            .join("\n");
        Err(anyhow::anyhow!(
            "Failed to update dependencies in the following directories:\n{}",
            error_messages
        ))
    } else {
        info!("Successfully updated dependencies");
        Ok(())
    }
}
