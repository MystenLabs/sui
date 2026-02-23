// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use std::path::Path;

use anyhow::{Context, Result, anyhow, bail};
use move_command_line_common::insta_assert;
use sui_forking::StartupSeeding;
use tokio::process::Command;

use crate::harness::forking_runtime::{ForkingHarness, binary_path};
use crate::harness::redaction::redact_snapshot_output;
use crate::harness::source_network::SourceNetworkHarness;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum ScriptMode {
    Prestarted,
    StartOnly,
}

pub async fn run_shell_script_snapshot(path: &Path) -> Result<()> {
    let script_contents = std::fs::read_to_string(path)
        .with_context(|| format!("failed to read shell script '{}'", path.display()))?;
    let mode = parse_script_mode(&script_contents)?;

    let sandbox_dir = tempfile::tempdir().context("failed to create shell test sandbox")?;
    let sandbox = sandbox_dir.path();
    let script_name = "script.sh";

    std::fs::write(sandbox.join(script_name), &script_contents)
        .with_context(|| format!("failed to stage script in '{}'", sandbox.display()))?;
    std::fs::copy(
        Path::new("tests/shell_tests/common.sh"),
        sandbox.join("common.sh"),
    )
    .with_context(|| format!("failed to stage common.sh in '{}'", sandbox.display()))?;

    let binary = binary_path()?;
    let mut source_graphql_url = String::new();
    let mut source_fullnode_url = String::new();
    let mut forking_server_url = String::new();
    let mut forking_rpc_endpoint = String::new();

    let mut maybe_runtime = None;
    let mut maybe_source = None;
    if mode == ScriptMode::Prestarted {
        let source = SourceNetworkHarness::fast().await?;
        source_graphql_url = source.graphql_url().to_string();
        source_fullnode_url = source.fullnode_url().to_string();

        let data_dir = sandbox.join("forking-data");
        let forking = ForkingHarness::start_programmatic(
            &source,
            source.fork_checkpoint(),
            StartupSeeding::None,
            data_dir,
        )
        .await?;

        forking_server_url = forking.base_url().to_string();
        forking_rpc_endpoint = forking.grpc_endpoint();
        maybe_runtime = Some(forking);
        maybe_source = Some(source);
    }

    let output = Command::new("bash")
        .arg("-euo")
        .arg("pipefail")
        .arg(script_name)
        .current_dir(sandbox)
        .env("SUI_FORKING_BIN", &binary)
        .env("FORKING_SERVER_URL", &forking_server_url)
        .env("FORKING_RPC_ENDPOINT", &forking_rpc_endpoint)
        .env("SOURCE_GRAPHQL_URL", &source_graphql_url)
        .env("SOURCE_FULLNODE_URL", &source_fullnode_url)
        .env("TEST_SANDBOX_DIR", sandbox)
        .output()
        .await
        .with_context(|| format!("failed to execute shell script '{}'", path.display()))?;

    if let Some(runtime) = maybe_runtime {
        runtime
            .shutdown()
            .await
            .context("failed to shutdown prestarted forking runtime")?;
    }
    drop(maybe_source);

    let snapshot = format!(
        "----- script -----\n{}\n----- mode -----\n{:?}\n----- results -----\nsuccess: {}\nexit_code: {}\n----- stdout -----\n{}\n----- stderr -----\n{}",
        script_contents,
        mode,
        output.status.success(),
        output.status.code().unwrap_or(!0),
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr),
    );

    let redacted = redact_snapshot_output(
        snapshot,
        sandbox,
        &[
            (&forking_server_url, "<FORKING_SERVER_URL>"),
            (&forking_rpc_endpoint, "<FORKING_RPC_ENDPOINT>"),
            (&source_graphql_url, "<SOURCE_GRAPHQL_URL>"),
            (&source_fullnode_url, "<SOURCE_FULLNODE_URL>"),
            (binary.to_string_lossy().as_ref(), "<SUI_FORKING_BIN>"),
        ],
    );

    insta_assert! {
        input_path: path,
        contents: redacted,
    };

    if !output.status.success() {
        bail!(
            "shell script '{}' exited with non-zero status {}",
            path.display(),
            output.status.code().unwrap_or(!0)
        );
    }

    Ok(())
}

fn parse_script_mode(script_contents: &str) -> Result<ScriptMode> {
    for line in script_contents.lines().take(8) {
        let trimmed = line.trim();
        if let Some(value) = trimmed.strip_prefix("# FORKING_TEST_MODE=") {
            return match value.trim() {
                "prestarted" => Ok(ScriptMode::Prestarted),
                "start_only" => Ok(ScriptMode::StartOnly),
                other => Err(anyhow!("unsupported FORKING_TEST_MODE '{other}'")),
            };
        }
    }

    Ok(ScriptMode::Prestarted)
}
