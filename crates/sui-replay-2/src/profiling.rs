// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

//! Bytecode profile dumping policy for the replay tool.
//!
//! The profiling counters in the Move VM accumulate monotonically over the
//! lifetime of a `MoveRuntime`. When replaying many transactions in one
//! process, the choice of *when* to dump matters; this module captures that
//! choice as an enum and applies it from the replay loop.
//!
//! # Modes
//!
//! Set `MOVE_VM_PROFILE_MODE` to one of:
//!
//! - `per-transaction` — reset counters before each transaction; emit a
//!   per-transaction snapshot afterwards. With `MOVE_VM_DUMP_PROFILE_FILE`
//!   set, the file is overwritten on each transaction (so the final file
//!   reflects the last replayed transaction only).
//! - `per-transaction-file` — same as above but the dump file path has the
//!   transaction digest spliced in as `<base>.<digest>.json`, producing one
//!   file per transaction.
//! - `end-of-replay` (default) — accumulate counts across the whole replay
//!   session and emit a single snapshot at the end. **Only meaningful with
//!   `--cache-executor`**: without executor caching each transaction creates
//!   and drops its own executor, so counters cannot survive across
//!   transactions and the end-of-session emission walks an empty cache.

use std::path::{Path, PathBuf};
use sui_execution::Executor;
use sui_execution::profiling::{MOVE_VM_DUMP_PROFILE_FILE_ENV, MOVE_VM_PROFILE_MODE_ENV};
use sui_types::digests::TransactionDigest;

/// Minimal interface the profile-mode dispatcher needs from an executor.
/// Lets us unit-test the per-mode wiring without standing up the full
/// `Executor` trait surface.
///
/// Production code uses [`ExecutorProfileSink`] to bridge an
/// `&dyn Executor` into this trait.
pub trait ProfileSink {
    fn emit_bytecode_profile(&self);
    fn reset_bytecode_profile(&self);
    #[cfg(feature = "tracing")]
    fn bytecode_profile_snapshot(&self) -> Option<sui_execution::profiling::BytecodeSnapshot>;
}

/// Adapter that forwards `ProfileSink` calls to the underlying `Executor`.
/// Avoids the lack of cross-trait upcasting in Rust (we can't cast
/// `&dyn Executor` to `&dyn ProfileSink` directly).
pub struct ExecutorProfileSink<'a>(pub &'a dyn Executor);

impl ProfileSink for ExecutorProfileSink<'_> {
    fn emit_bytecode_profile(&self) {
        self.0.emit_bytecode_profile();
    }
    fn reset_bytecode_profile(&self) {
        self.0.reset_bytecode_profile();
    }
    #[cfg(feature = "tracing")]
    fn bytecode_profile_snapshot(&self) -> Option<sui_execution::profiling::BytecodeSnapshot> {
        self.0.bytecode_profile_snapshot()
    }
}

/// Where bytecode profile snapshots get emitted relative to transaction
/// boundaries during replay.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum BytecodeProfileMode {
    /// Reset before each transaction; dump after each transaction (file gets
    /// overwritten on every transaction).
    PerTransaction,
    /// Reset before each transaction; write each per-transaction snapshot to
    /// `<base>.<digest>.json`.
    PerTransactionFile,
    /// Accumulate across the entire replay session; emit once when the
    /// session ends.
    ///
    /// Requires `--cache-executor` to do anything useful: counters live
    /// inside the executor, so without caching each transaction's counters
    /// are dropped before the session-end hook runs.
    #[default]
    EndOfReplay,
}

impl BytecodeProfileMode {
    /// Parse the `MOVE_VM_PROFILE_MODE` env var; returns the default if unset
    /// or unrecognised. An unrecognised value is logged at warn level so it
    /// doesn't fail silently.
    ///
    /// The parsed value is cached in a process-wide `OnceLock`, so callers
    /// that hit this on every transaction pay the env lookup (which takes a
    /// global OS lock on Unix) exactly once per process.
    pub fn from_env() -> Self {
        static CACHED: std::sync::OnceLock<BytecodeProfileMode> = std::sync::OnceLock::new();
        *CACHED.get_or_init(Self::parse_env)
    }

    /// Parse the env var without caching. Internal helper for `from_env`.
    fn parse_env() -> Self {
        let Ok(raw) = std::env::var(MOVE_VM_PROFILE_MODE_ENV) else {
            return Self::default();
        };
        match raw.to_ascii_lowercase().as_str() {
            "per-transaction" | "per_transaction" | "pertx" => Self::PerTransaction,
            "per-transaction-file" | "per_transaction_file" | "pertxfile" => {
                Self::PerTransactionFile
            }
            "end-of-replay" | "end_of_replay" | "end" | "session" => Self::EndOfReplay,
            other => {
                tracing::warn!(
                    %other,
                    default = ?Self::default(),
                    "unknown {MOVE_VM_PROFILE_MODE_ENV} value; falling back to default",
                );
                Self::default()
            }
        }
    }

    /// Hook called before a transaction begins executing.
    pub fn before_transaction(self, executor: &dyn ProfileSink) {
        match self {
            Self::PerTransaction | Self::PerTransactionFile => {
                executor.reset_bytecode_profile();
            }
            Self::EndOfReplay => {} // accumulate across transactions
        }
    }

    /// Hook called after a transaction finishes executing.
    #[cfg(feature = "tracing")]
    pub fn after_transaction(self, executor: &dyn ProfileSink, digest: &TransactionDigest) {
        match self {
            Self::PerTransaction => executor.emit_bytecode_profile(),
            Self::PerTransactionFile => {
                if let Some(snapshot) = executor.bytecode_profile_snapshot() {
                    if let Some(base) = dump_file_base() {
                        snapshot.dump_to_file(per_tx_path(base, digest));
                    } else {
                        // No base path configured — fall back to the
                        // tracing log so the data is not silently dropped.
                        tracing::info!(
                            total = snapshot.total(),
                            profile = %snapshot.format_csv(),
                            %digest,
                            "move-vm bytecode profile",
                        );
                    }
                }
            }
            Self::EndOfReplay => {} // wait for end_of_session
        }
    }

    #[cfg(not(feature = "tracing"))]
    pub fn after_transaction(self, _executor: &dyn ProfileSink, _digest: &TransactionDigest) {}

    /// Hook called once when the replay session ends.
    pub fn end_of_session(self, executor: &dyn ProfileSink) {
        if matches!(self, Self::EndOfReplay) {
            executor.emit_bytecode_profile();
        }
    }
}

/// RAII guard that pairs `before_transaction` (on construction) with
/// `after_transaction` (on drop). Ensures the after-hook runs even when
/// the surrounding code returns early via `?` or panics — without it,
/// per-transaction modes would leak counters from the failed tx into
/// the next, and PerTransactionFile would silently miss the dump.
pub struct ProfileGuard<'a> {
    mode: BytecodeProfileMode,
    sink: &'a dyn ProfileSink,
    digest: TransactionDigest,
}

impl<'a> ProfileGuard<'a> {
    /// Run the before-transaction hook and return a guard that will run
    /// the after-transaction hook on drop.
    pub fn enter(
        mode: BytecodeProfileMode,
        sink: &'a dyn ProfileSink,
        digest: TransactionDigest,
    ) -> Self {
        mode.before_transaction(sink);
        Self { mode, sink, digest }
    }
}

impl Drop for ProfileGuard<'_> {
    fn drop(&mut self) {
        self.mode.after_transaction(self.sink, &self.digest);
    }
}

/// Cached `MOVE_VM_DUMP_PROFILE_FILE` value. Looked up once per process to
/// avoid repeated `std::env::var` calls (which take a global OS lock on Unix)
/// from the per-transaction hooks.
fn dump_file_base() -> Option<&'static Path> {
    static CACHED: std::sync::OnceLock<Option<PathBuf>> = std::sync::OnceLock::new();
    CACHED
        .get_or_init(|| {
            std::env::var(MOVE_VM_DUMP_PROFILE_FILE_ENV)
                .ok()
                .map(PathBuf::from)
        })
        .as_deref()
}

/// Splice a transaction digest into a dump file path:
/// `<stem>.<digest>.<ext>` next to the base path. If the input has no
/// extension, defaults to `.json`. Pure function — no env access — so it is
/// directly testable.
fn per_tx_path(base: &Path, digest: &TransactionDigest) -> PathBuf {
    let stem = base
        .file_stem()
        .and_then(|s| s.to_str())
        .unwrap_or("profile");
    let ext = base.extension().and_then(|e| e.to_str()).unwrap_or("json");
    let parent = base.parent().filter(|p| !p.as_os_str().is_empty());
    let filename = format!("{}.{}.{}", stem, digest, ext);
    match parent {
        Some(p) => p.join(filename),
        None => PathBuf::from(filename),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::{AtomicUsize, Ordering};

    /// Tiny fake that records which `ProfileSink` hooks were called so we
    /// can verify the per-mode dispatch logic without standing up a full VM.
    struct RecordingSink {
        emit_calls: AtomicUsize,
        reset_calls: AtomicUsize,
        snapshot_calls: AtomicUsize,
    }

    impl RecordingSink {
        fn new() -> Self {
            Self {
                emit_calls: AtomicUsize::new(0),
                reset_calls: AtomicUsize::new(0),
                snapshot_calls: AtomicUsize::new(0),
            }
        }
    }

    impl ProfileSink for RecordingSink {
        fn emit_bytecode_profile(&self) {
            self.emit_calls.fetch_add(1, Ordering::SeqCst);
        }
        fn reset_bytecode_profile(&self) {
            self.reset_calls.fetch_add(1, Ordering::SeqCst);
        }
        #[cfg(feature = "tracing")]
        fn bytecode_profile_snapshot(&self) -> Option<sui_execution::profiling::BytecodeSnapshot> {
            self.snapshot_calls.fetch_add(1, Ordering::SeqCst);
            None
        }
    }

    #[test]
    fn test_per_tx_path_with_extension() {
        let digest = TransactionDigest::ZERO;
        let result = per_tx_path(Path::new("/tmp/profile.json"), &digest);
        assert_eq!(
            result.to_string_lossy(),
            format!("/tmp/profile.{}.json", digest)
        );
    }

    #[test]
    fn test_per_tx_path_no_extension() {
        let digest = TransactionDigest::ZERO;
        let result = per_tx_path(Path::new("/tmp/profile"), &digest);
        // Defaults to .json when the input has no extension.
        assert_eq!(
            result.to_string_lossy(),
            format!("/tmp/profile.{}.json", digest)
        );
    }

    #[test]
    fn test_per_tx_path_bare_filename() {
        // No parent directory in the input — output is just a filename.
        let digest = TransactionDigest::ZERO;
        let result = per_tx_path(Path::new("profile.json"), &digest);
        assert_eq!(result.to_string_lossy(), format!("profile.{}.json", digest));
    }

    #[test]
    fn test_default_mode() {
        assert_eq!(
            BytecodeProfileMode::default(),
            BytecodeProfileMode::EndOfReplay
        );
    }

    /// `PerTransaction` mode: reset on entry, emit on exit, no end-of-session
    /// emission. Only runs with the `tracing` feature, which is the only
    /// configuration where the per-tx hooks actually do anything.
    #[test]
    #[cfg(feature = "tracing")]
    fn test_per_transaction_mode_calls() {
        let exec = RecordingSink::new();
        let digest = TransactionDigest::ZERO;
        let mode = BytecodeProfileMode::PerTransaction;

        mode.before_transaction(&exec);
        mode.after_transaction(&exec, &digest);
        mode.end_of_session(&exec);

        assert_eq!(exec.reset_calls.load(Ordering::SeqCst), 1);
        assert_eq!(exec.emit_calls.load(Ordering::SeqCst), 1);
    }

    /// `PerTransactionFile` mode: reset on entry, snapshot on exit, no
    /// end-of-session emission.
    #[test]
    #[cfg(feature = "tracing")]
    fn test_per_transaction_file_mode_calls() {
        let exec = RecordingSink::new();
        let digest = TransactionDigest::ZERO;
        let mode = BytecodeProfileMode::PerTransactionFile;

        mode.before_transaction(&exec);
        mode.after_transaction(&exec, &digest);
        mode.end_of_session(&exec);

        assert_eq!(exec.reset_calls.load(Ordering::SeqCst), 1);
        assert_eq!(exec.snapshot_calls.load(Ordering::SeqCst), 1);
        assert_eq!(exec.emit_calls.load(Ordering::SeqCst), 0);
    }

    /// `EndOfReplay` mode: no per-tx resets or emissions, single emit at
    /// session end.
    #[test]
    fn test_end_of_replay_mode_calls() {
        let exec = RecordingSink::new();
        let digest = TransactionDigest::ZERO;
        let mode = BytecodeProfileMode::EndOfReplay;

        mode.before_transaction(&exec);
        mode.after_transaction(&exec, &digest);
        mode.before_transaction(&exec);
        mode.after_transaction(&exec, &digest);
        // Only one end_of_session call across multiple txns.
        mode.end_of_session(&exec);

        assert_eq!(exec.reset_calls.load(Ordering::SeqCst), 0);
        assert_eq!(exec.emit_calls.load(Ordering::SeqCst), 1);
    }

    /// `ProfileGuard::enter` runs `before_transaction`; the guard's `Drop`
    /// runs `after_transaction`. Verifies the pair fires correctly on a
    /// happy-path scope exit.
    #[test]
    fn test_profile_guard_pairs_hooks() {
        let exec = RecordingSink::new();
        let digest = TransactionDigest::ZERO;

        {
            let _g = ProfileGuard::enter(BytecodeProfileMode::PerTransaction, &exec, digest);
            // Scope ends here; Drop fires after_transaction.
        }

        // PerTransaction: 1 reset on entry, 1 emit on drop.
        assert_eq!(exec.reset_calls.load(Ordering::SeqCst), 1);
        assert_eq!(exec.emit_calls.load(Ordering::SeqCst), 1);
    }

    /// The whole point of the guard: `after_transaction` must run even if
    /// the surrounding code returns early via `?` or panics.
    #[test]
    fn test_profile_guard_runs_after_hook_on_early_return() {
        let exec = RecordingSink::new();
        let digest = TransactionDigest::ZERO;

        // Helper that takes the guard, then early-returns.
        fn maybe_fail(exec: &RecordingSink, digest: TransactionDigest) -> Result<(), &'static str> {
            let _g = ProfileGuard::enter(BytecodeProfileMode::PerTransaction, exec, digest);
            // Pretend the executor or post-checks errored.
            Err("simulated failure")
        }

        let res = maybe_fail(&exec, digest);
        assert!(res.is_err());

        // Even though we returned Err, the guard's Drop ran the after-hook.
        assert_eq!(exec.reset_calls.load(Ordering::SeqCst), 1);
        assert_eq!(exec.emit_calls.load(Ordering::SeqCst), 1);
    }
}
