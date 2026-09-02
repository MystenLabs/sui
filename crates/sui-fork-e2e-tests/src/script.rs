// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

//! Run a shell script in its own process group, capturing output and reaping stray children.
//!
//! The Sui CLI shell harness calls `Command::output`, which returns only when both pipes reach end
//! of file. A script here backgrounds `sui-fork start`, and any background process that still
//! holds the script's stdout or stderr keeps `output` blocked after `bash` has exited. The runner
//! therefore places `bash` in a fresh process group, waits for `bash` itself under a deadline, and
//! signals the whole group before joining the pipe readers, so a script that forgets to stop its
//! fork can neither hang nor leak the test. Because the group is separate from the harness's own,
//! [`forward_termination_signals`] relays Ctrl-C and nextest's SIGTERM into it.

use std::io::Read;
use std::process::Command;
use std::process::ExitStatus;
use std::process::Stdio;
use std::sync::Mutex;
use std::thread;
use std::time::Duration;
use std::time::Instant;

use anyhow::Context;
use anyhow::Result;

/// Time between the SIGTERM and the SIGKILL sent to a script's process group.
const KILL_GRACE: Duration = Duration::from_secs(5);

/// Poll interval while waiting for `bash` to exit or for a signalled group to disappear.
const POLL: Duration = Duration::from_millis(50);

/// Process groups of scripts running in this process, so a termination signal can reach them.
static RUNNING_GROUPS: Mutex<Vec<u32>> = Mutex::new(Vec::new());

/// Captured result of one script run.
pub struct ScriptOutput {
    pub status: ExitStatus,
    pub stdout: Vec<u8>,
    pub stderr: Vec<u8>,
    /// Whether the harness killed the script because it exceeded its deadline.
    pub timed_out: bool,
}

/// Run `command` in a new process group and collect its output within `timeout`.
///
/// Returns once `bash` has exited and every other process in the group has been terminated, so
/// the returned output is complete. A script that exceeds `timeout` is killed and reported with
/// `timed_out` set, together with whatever it printed before the kill.
pub fn run_script(mut command: Command, timeout: Duration) -> Result<ScriptOutput> {
    command
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());
    #[cfg(unix)]
    {
        use std::os::unix::process::CommandExt;
        command.process_group(0);
    }

    let mut child = command.spawn().context("failed to spawn the script")?;
    let group = child.id();
    RUNNING_GROUPS.lock().unwrap().push(group);

    let stdout = spawn_reader(child.stdout.take().expect("stdout is piped"));
    let stderr = spawn_reader(child.stderr.take().expect("stderr is piped"));

    let deadline = Instant::now() + timeout;
    let (status, timed_out) = loop {
        if let Some(status) = child.try_wait().context("failed to wait for the script")? {
            break (status, false);
        }
        if Instant::now() >= deadline {
            terminate_group(group);
            let status = child
                .wait()
                .context("failed to wait for the killed script")?;
            break (status, true);
        }
        thread::sleep(POLL);
    };

    // `bash` is gone, so whatever remains in the group is a background process the script did
    // not stop. Kill it before joining the readers because it may still hold the pipes open.
    terminate_group(group);
    RUNNING_GROUPS
        .lock()
        .unwrap()
        .retain(|running| *running != group);

    Ok(ScriptOutput {
        status,
        stdout: stdout.join().expect("stdout reader panicked")?,
        stderr: stderr.join().expect("stderr reader panicked")?,
        timed_out,
    })
}

/// Relay SIGINT and SIGTERM to every running script's process group, then exit.
///
/// Spawn the future on the test's runtime before running a script. Without it, a Ctrl-C at the
/// terminal or a nextest timeout ends the harness process and leaves `bash` and its fork running,
/// because they live in a process group the signal never reaches.
pub async fn forward_termination_signals() {
    #[cfg(unix)]
    {
        use tokio::signal::unix::SignalKind;
        use tokio::signal::unix::signal;

        let mut interrupt = signal(SignalKind::interrupt()).expect("SIGINT handler");
        let mut terminate = signal(SignalKind::terminate()).expect("SIGTERM handler");
        let code = tokio::select! {
            _ = interrupt.recv() => 130,
            _ = terminate.recv() => 143,
        };
        for group in RUNNING_GROUPS.lock().unwrap().drain(..) {
            terminate_group(group);
        }
        std::process::exit(code);
    }
}

fn spawn_reader<R: Read + Send + 'static>(mut reader: R) -> thread::JoinHandle<Result<Vec<u8>>> {
    thread::spawn(move || {
        let mut buffer = Vec::new();
        reader
            .read_to_end(&mut buffer)
            .context("failed to read script output")?;
        Ok(buffer)
    })
}

/// Send SIGTERM to the group, wait up to [`KILL_GRACE`] for it to empty, then SIGKILL survivors.
///
/// SIGTERM goes first because `sui-fork start` shuts down cleanly on it, which keeps its data
/// directory consistent for scripts that inspect it afterwards.
fn terminate_group(group: u32) {
    if !signal_group(group, "TERM") {
        return;
    }
    let deadline = Instant::now() + KILL_GRACE;
    while Instant::now() < deadline && signal_group(group, "0") {
        thread::sleep(POLL);
    }
    signal_group(group, "KILL");
}

/// Signal every process in `group` and return whether any process received the signal.
///
/// The signal goes through bash's `kill` builtin, which accepts a negative process group id on
/// both macOS and Linux, so the crate needs neither `libc` nor an `unsafe` block.
fn signal_group(group: u32, signal: &str) -> bool {
    Command::new("bash")
        .args(["-c", r#"kill -s "$1" -- "-$2""#, "kill", signal])
        .arg(group.to_string())
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .status()
        .is_ok_and(|status| status.success())
}
