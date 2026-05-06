// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use anyhow::{Context, bail};
use clap::Parser;
use std::{
    io::{self, BufRead, IsTerminal, Write},
    process::{Command, Stdio},
};

/// Format Move source files using `prettier-move`.
///
/// This is a thin alias around the `prettier-move` shim shipped by the
/// `@mysten/prettier-plugin-move` npm package — every argument is forwarded
/// verbatim. If `prettier-move` is not found on `PATH`, the command offers
/// to install it via `npm i -g`.
#[derive(Parser)]
#[group(id = "sui-move-format")]
pub struct Format {
    /// Arguments forwarded verbatim to `prettier-move`. Examples:
    ///   sui move format -c sources/foo.move      # check
    ///   sui move format -w .                     # write the package
    #[clap(trailing_var_arg = true, allow_hyphen_values = true)]
    args: Vec<String>,
}

const NPM_INSTALL_ARGS: &[&str] = &["i", "-g", "prettier", "@mysten/prettier-plugin-move"];

impl Format {
    pub async fn execute(self) -> anyhow::Result<()> {
        if !is_on_path("prettier-move")? {
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

/// Probe `PATH` for `bin` by invoking `bin --version`. Returns `Ok(true)` if
/// the binary ran (regardless of its exit status — `--version` may be
/// rejected by some shims), `Ok(false)` if the binary was not found, and
/// `Err` for other I/O errors.
fn is_on_path(bin: &str) -> anyhow::Result<bool> {
    match Command::new(bin)
        .arg("--version")
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .status()
    {
        Ok(_) => Ok(true),
        Err(e) if e.kind() == io::ErrorKind::NotFound => Ok(false),
        Err(e) => Err(e).with_context(|| format!("failed to probe `{bin}`")),
    }
}

fn bootstrap_prettier_move() -> anyhow::Result<()> {
    if !is_on_path("npm")? {
        bail!(
            "`prettier-move` is not installed and `npm` was not found on PATH.\n\
             Install Node.js 18+ (which provides `npm`) from https://nodejs.org, \
             then re-run `sui move format`."
        );
    }

    let install_cmd = format!("npm {}", NPM_INSTALL_ARGS.join(" "));

    if !io::stdin().is_terminal() {
        bail!(
            "`prettier-move` is not installed. Re-run from a terminal to install \
             interactively, or install manually with:\n    {install_cmd}"
        );
    }

    print!("`prettier-move` is not installed. Install it now with `{install_cmd}`? [y/N] ");
    io::stdout().flush().ok();

    let mut answer = String::new();
    io::stdin()
        .lock()
        .read_line(&mut answer)
        .context("failed to read response from stdin")?;

    if !matches!(answer.trim(), "y" | "Y" | "yes" | "YES") {
        bail!("aborted: `prettier-move` is required for `sui move format`");
    }

    let status = Command::new("npm")
        .args(NPM_INSTALL_ARGS)
        .stdin(Stdio::inherit())
        .stdout(Stdio::inherit())
        .stderr(Stdio::inherit())
        .status()
        .context("failed to spawn `npm`")?;

    if !status.success() {
        bail!("`{install_cmd}` failed with status {status}");
    }

    if !is_on_path("prettier-move")? {
        bail!(
            "`npm` reported success but `prettier-move` is still not on PATH. \
             Ensure your global npm `bin` directory is on PATH (try `npm bin -g`)."
        );
    }

    eprintln!("\nSuccessfully installed `prettier-move`.\n");
    Ok(())
}
