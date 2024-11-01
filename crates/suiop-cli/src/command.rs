// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use anyhow::anyhow;
use anyhow::Context;
use anyhow::Result;
use spinners::Spinner;
use spinners::Spinners;
use std::process::Command;
use std::process::Output;
use std::process::Stdio;
use tracing::debug;
const SPINNER: Spinners = Spinners::Dots12;

#[derive(Debug, Clone)]
pub struct CommandOptions {
    shared_stdio: bool,
    show_spinner: bool,
}

impl CommandOptions {
    pub fn new(shared_stdio: bool, show_spinner: bool) -> Self {
        CommandOptions {
            shared_stdio,
            show_spinner,
        }
    }
}

impl Default for CommandOptions {
    fn default() -> Self {
        CommandOptions {
            shared_stdio: false,
            show_spinner: true,
        }
    }
}

pub fn run_cmd(cmd_in: Vec<&str>, options: Option<CommandOptions>) -> Result<Output> {
    debug!("attempting to run {}", cmd_in.join(" "));
    let opts = options.unwrap_or_default();

    let mut cmd = Command::new(cmd_in[0]);
    // add extra args
    let cmd = if cmd_in.len() > 1 {
        cmd.args(cmd_in[1..].iter())
    } else {
        &mut cmd
    };
    // add stdio
    let cmd = if opts.shared_stdio {
        debug!("stdio will be shared between parent shell and command process");
        cmd.stdout(Stdio::inherit()).stdin(Stdio::inherit())
    } else {
        cmd
    };
    let res = if opts.show_spinner {
        let mut sp = Spinner::new(SPINNER, "".into());
        let result = cmd.output().context(format!(
            "failed to run command with spinner {}",
            cmd_in.join(" ")
        ))?;
        sp.stop();
        print!("\r");
        result
    } else {
        cmd.output()
            .context(format!("failed to run command {}", cmd_in.join(" ")))?
    };

    if !res.status.success() {
        Err(anyhow!(
            "command failed to run: {}",
            std::str::from_utf8(&res.stderr)?
        ))
    } else {
        Ok(res)
    }
}
