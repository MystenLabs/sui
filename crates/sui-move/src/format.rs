// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use anyhow::{Context, bail, ensure};
use clap::Parser;
use std::{
    io::{self, BufRead, IsTerminal, Write},
    process::{Command, Stdio},
};

/// Format Move source files using `prettier-move`.
///
/// This is a thin alias around the `prettier-move` shim shipped by the
/// `@mysten/prettier-plugin-move` npm package — every argument (including
/// `--help`) is forwarded verbatim. If `prettier-move` is not found on
/// `PATH`, the command offers to install it via `npm i -g`.
#[derive(Parser)]
#[clap(disable_help_flag = true)]
#[group(id = "sui-move-format")]
pub struct Format {
    /// Arguments forwarded verbatim to `prettier-move`. Examples:
    ///   sui move format -c sources/foo.move      # check
    ///   sui move format -w .                     # write the package
    ///   sui move format --help                   # prettier-move help
    #[clap(trailing_var_arg = true, allow_hyphen_values = true)]
    args: Vec<String>,
}

const NPM_INSTALL_ARGS: &[&str] = &["i", "-g", "prettier", "@mysten/prettier-plugin-move"];

impl Format {
    pub async fn execute(self) -> anyhow::Result<()> {
        if which::which("prettier-move").is_err() {
            bootstrap_prettier_move()?;
        }

        let status = Command::new("prettier-move")
            .args(&self.args)
            .stdin(Stdio::inherit())
            .stdout(Stdio::inherit())
            .stderr(Stdio::inherit())
            .status()
            .context("failed to spawn `prettier-move`")?;

        if let Some(code) = status.code() {
            std::process::exit(code);
        }
        bail!("`prettier-move` terminated by signal");
    }
}

fn bootstrap_prettier_move() -> anyhow::Result<()> {
    let install_cmd = format!("npm {}", NPM_INSTALL_ARGS.join(" "));

    ensure!(
        which::which("npm").is_ok(),
        "`prettier-move` is not installed and `npm` was not found on PATH.\n\
         Install Node.js 18+ (which provides `npm`) from https://nodejs.org, \
         then install `prettier-move` with:\n    {install_cmd}"
    );

    ensure!(
        io::stdin().is_terminal(),
        "`prettier-move` is not installed. Re-run from a terminal to install \
         interactively, or install manually with:\n    {install_cmd}"
    );

    print!("`prettier-move` is not installed. Install it now with `{install_cmd}`? [y/N] ");
    io::stdout().flush().ok();

    let mut answer = String::new();
    io::stdin()
        .lock()
        .read_line(&mut answer)
        .context("failed to read response from stdin")?;

    ensure!(
        matches!(answer.trim(), "y" | "Y" | "yes" | "YES"),
        "aborted: `prettier-move` is required for `sui move format`"
    );

    let status = Command::new("npm")
        .args(NPM_INSTALL_ARGS)
        .stdin(Stdio::inherit())
        .stdout(Stdio::inherit())
        .stderr(Stdio::inherit())
        .status()
        .context("failed to spawn `npm`")?;

    ensure!(
        status.success(),
        "`{install_cmd}` failed with status {status}"
    );

    ensure!(
        which::which("prettier-move").is_ok(),
        "`npm` reported success but `prettier-move` is still not on PATH. \
         Ensure your global npm `bin` directory is on PATH (try `npm bin -g`)."
    );

    eprintln!("\nSuccessfully installed `prettier-move`.\n");
    Ok(())
}
