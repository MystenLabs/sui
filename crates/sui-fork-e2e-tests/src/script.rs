// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

//! Run a child in its own process group, so that it and everything it spawns can be reaped.
//!
//! The Sui CLI shell harness calls `Command::output`, which returns only when both pipes reach end
//! of file. A script here backgrounds `sui-fork start`, and any background process that still
//! holds the script's stdout or stderr keeps `output` blocked after `bash` has exited. The runner
//! therefore places `bash` in a fresh process group, waits for `bash` itself under a deadline, and
//! signals the whole group before joining the pipe readers, so a script that forgets to stop its
//! fork can neither hang nor leak the test. The source network's `sui start`, which spawns a
//! PostgreSQL of its own, is placed in a group the same way. Because those groups are separate
//! from the harness's own, [`run_until_terminated`] relays Ctrl-C and nextest's SIGTERM into
//! them.

use std::fmt;
use std::future::Future;
use std::io::Read;
use std::process::Child;
use std::process::Command;
use std::process::ExitStatus;
use std::process::Stdio;
use std::sync::Mutex;
use std::sync::atomic::AtomicU8;
use std::sync::atomic::Ordering;
use std::thread;
use std::time::Duration;
use std::time::Instant;

use anyhow::Context;
use anyhow::Result;

/// Time between the SIGTERM and the SIGKILL sent to a process group.
const KILL_GRACE: Duration = Duration::from_secs(5);

/// Poll interval while waiting for `bash` to exit or for a signalled group to disappear.
const POLL: Duration = Duration::from_millis(50);

const NO_TERMINATION_SIGNAL: u8 = 0;

/// Process groups spawned by this process and not yet released, so a termination signal can reach
/// them. Later entries depend on earlier ones: a script talks to the network started before it.
static RUNNING_GROUPS: Mutex<Vec<u32>> = Mutex::new(Vec::new());

/// The first termination signal this process received. A subsequent test in the same process must
/// stop immediately rather than start another source network after the signal has been handled.
static RECEIVED_TERMINATION_SIGNAL: AtomicU8 = AtomicU8::new(NO_TERMINATION_SIGNAL);

/// Signal that requested termination of the test process.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum TerminationSignal {
    Interrupt,
    Terminate,
}

/// Captured result of one script run.
pub struct ScriptOutput {
    pub status: ExitStatus,
    pub stdout: Vec<u8>,
    pub stderr: Vec<u8>,
    /// Whether the harness killed the script because it exceeded its deadline.
    pub timed_out: bool,
}

impl TerminationSignal {
    fn code(self) -> u8 {
        match self {
            Self::Interrupt => 1,
            Self::Terminate => 2,
        }
    }

    fn from_code(code: u8) -> Option<Self> {
        match code {
            1 => Some(Self::Interrupt),
            2 => Some(Self::Terminate),
            _ => None,
        }
    }
}

impl fmt::Display for TerminationSignal {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Interrupt => f.write_str("SIGINT"),
            Self::Terminate => f.write_str("SIGTERM"),
        }
    }
}

/// Run `operation` until it completes or the process receives SIGINT or SIGTERM.
///
/// A signal stops the registered process groups and drops `operation` before returning. Tests in
/// the same process also stop without polling `operation` after the first signal.
pub async fn run_until_terminated<F>(
    operation: F,
) -> std::result::Result<F::Output, TerminationSignal>
where
    F: Future,
{
    if let Some(signal) = received_termination_signal() {
        return Err(signal);
    }

    let mut termination = tokio::spawn(forward_termination_signals());
    tokio::pin!(operation);
    tokio::select! {
        biased;
        signal = &mut termination => {
            Err(signal.expect("termination signal task panicked"))
        }
        output = &mut operation => {
            termination.abort();
            Ok(output)
        }
    }
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
    let mut child = spawn_in_group(&mut command).context("failed to spawn the script")?;
    let group = child.id();

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
    release_group(group);

    Ok(ScriptOutput {
        status,
        stdout: stdout.join().expect("stdout reader panicked")?,
        stderr: stderr.join().expect("stderr reader panicked")?,
        timed_out,
    })
}

/// Relay SIGINT and SIGTERM to every running process group and return the received signal.
///
/// Run this future alongside the test operation. When it returns, drop the operation before
/// exiting so its destructors remove the temporary directories. Groups go in reverse order so
/// that a script dies before the network it talks to.
async fn forward_termination_signals() -> TerminationSignal {
    #[cfg(unix)]
    {
        use tokio::signal::unix::SignalKind;
        use tokio::signal::unix::signal;

        if let Some(signal) = received_termination_signal() {
            terminate_running_groups();
            return signal;
        }

        let mut interrupt = signal(SignalKind::interrupt()).expect("SIGINT handler");
        let mut terminate = signal(SignalKind::terminate()).expect("SIGTERM handler");
        let signal = tokio::select! {
            _ = interrupt.recv() => TerminationSignal::Interrupt,
            _ = terminate.recv() => TerminationSignal::Terminate,
        };
        let signal = match RECEIVED_TERMINATION_SIGNAL.compare_exchange(
            NO_TERMINATION_SIGNAL,
            signal.code(),
            Ordering::AcqRel,
            Ordering::Acquire,
        ) {
            Ok(_) => signal,
            Err(code) => {
                TerminationSignal::from_code(code).expect("stored termination signal is valid")
            }
        };
        terminate_running_groups();
        signal
    }
    #[cfg(not(unix))]
    std::future::pending().await
}

/// Spawn `command` as the leader of a new process group and register it for signal handling by
/// [`run_until_terminated`], until [`release_group`] is called with the child's id.
pub(crate) fn spawn_in_group(command: &mut Command) -> Result<Child> {
    #[cfg(unix)]
    {
        use std::os::unix::process::CommandExt;
        command.process_group(0);
    }
    let child = command.spawn()?;
    RUNNING_GROUPS.lock().unwrap().push(child.id());
    Ok(child)
}

/// Terminate whatever is left in `group` and forget it.
pub(crate) fn release_group(group: u32) {
    terminate_group(group);
    RUNNING_GROUPS
        .lock()
        .unwrap()
        .retain(|running| *running != group);
}

/// Signal one process by id and return whether it received the signal.
pub(crate) fn signal_process(pid: u32, signal: &str) -> bool {
    kill(&pid.to_string(), signal)
}

fn received_termination_signal() -> Option<TerminationSignal> {
    TerminationSignal::from_code(RECEIVED_TERMINATION_SIGNAL.load(Ordering::Acquire))
}

fn terminate_running_groups() {
    for group in RUNNING_GROUPS.lock().unwrap().drain(..).rev() {
        terminate_group(group);
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
/// directory consistent for scripts that inspect it afterwards. `sui start` only handles SIGINT,
/// which its owner sends before falling back to this.
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
fn signal_group(group: u32, signal: &str) -> bool {
    kill(&format!("-{group}"), signal)
}

/// Run `kill -s <signal> -- <target>` through bash's builtin, which accepts a negative process
/// group id on both macOS and Linux, so the crate needs neither `libc` nor an `unsafe` block.
fn kill(target: &str, signal: &str) -> bool {
    Command::new("bash")
        .args(["-c", r#"kill -s "$1" -- "$2""#, "kill", signal, target])
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .status()
        .is_ok_and(|status| status.success())
}

#[cfg(all(test, unix))]
mod tests {
    use std::future::pending;
    use std::path::Path;
    use std::process::Child;
    use std::process::Command;
    use std::time::Duration;
    use std::time::Instant;

    use anyhow::Context;
    use anyhow::Result;
    use anyhow::bail;
    use anyhow::ensure;

    use super::TerminationSignal;
    use super::run_until_terminated;
    use super::signal_process;

    const HELPER_ROOT_ENV: &str = "SUI_FORK_E2E_TERMINATION_HELPER_ROOT";
    const HELPER_TEST: &str = "script::tests::termination_signal_drops_temporary_directories";
    const HELPER_TIMEOUT: Duration = Duration::from_secs(10);

    fn run_termination_helper(root: &Path) -> Result<()> {
        tokio::runtime::Runtime::new()?.block_on(async {
            let operation = async {
                let _directory = tempfile::tempdir_in(root)?;
                tokio::time::sleep(Duration::from_millis(100)).await;
                std::fs::write(root.join("ready"), [])?;
                pending::<()>().await;
                Ok::<(), anyhow::Error>(())
            };
            let received = run_until_terminated(operation)
                .await
                .expect_err("operation completed without a signal");
            ensure!(
                received == TerminationSignal::Terminate,
                "received {received}"
            );
            let repeated = tokio::time::timeout(
                Duration::from_secs(1),
                run_until_terminated(pending::<()>()),
            )
            .await
            .context("a subsequent test did not stop")?
            .expect_err("subsequent operation started");
            ensure!(repeated == received, "received {repeated}");
            Ok(())
        })
    }

    fn wait_for_ready(path: &Path, child: &mut Child) -> Result<()> {
        let deadline = Instant::now() + HELPER_TIMEOUT;
        while !path.exists() {
            if let Some(status) = child.try_wait()? {
                bail!("termination helper exited before it was ready with {status}");
            }
            if Instant::now() >= deadline {
                child.kill()?;
                child.wait()?;
                bail!("termination helper was not ready within {HELPER_TIMEOUT:?}");
            }
            std::thread::sleep(Duration::from_millis(10));
        }
        Ok(())
    }

    fn wait_for_exit(child: &mut Child) -> Result<std::process::ExitStatus> {
        let deadline = Instant::now() + HELPER_TIMEOUT;
        loop {
            if let Some(status) = child.try_wait()? {
                return Ok(status);
            }
            if Instant::now() >= deadline {
                child.kill()?;
                child.wait()?;
                bail!("termination helper did not exit within {HELPER_TIMEOUT:?}");
            }
            std::thread::sleep(Duration::from_millis(10));
        }
    }

    // Real subprocesses and OS signals bypass msim, and the helper needs a real Tokio runtime.
    #[test]
    #[cfg_attr(msim, ignore)]
    fn termination_signal_drops_temporary_directories() -> Result<()> {
        if let Some(root) = std::env::var_os(HELPER_ROOT_ENV) {
            return run_termination_helper(Path::new(&root));
        }

        let root = tempfile::tempdir()?;
        let mut child = Command::new(std::env::current_exe()?)
            .args(["--exact", HELPER_TEST, "--nocapture"])
            .env(HELPER_ROOT_ENV, root.path())
            .spawn()
            .context("failed to spawn termination helper")?;
        wait_for_ready(&root.path().join("ready"), &mut child)?;
        let child_directory = std::fs::read_dir(root.path())?
            .filter_map(|entry| entry.ok())
            .find(|entry| entry.path().is_dir())
            .context("termination helper did not create its temporary directory")?
            .path();

        if !signal_process(child.id(), "TERM") {
            child.kill()?;
            child.wait()?;
            bail!("failed to send SIGTERM to the termination helper");
        }
        let status = wait_for_exit(&mut child)?;

        ensure!(
            !child_directory.exists(),
            "temporary directory survived termination: {}",
            child_directory.display()
        );
        ensure!(status.success(), "termination helper failed with {status}");
        Ok(())
    }
}
