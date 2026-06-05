// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

//! Crash-recovery support: detect which transaction was executing when the process panicked.
//!
//! # How it works
//!
//! 1. At process startup, `install_panic_hook` chains a closure into the existing panic hook.
//!    When a panic fires, the hook reads a thread-local slot.  If that slot holds a transaction
//!    digest it appends the digest (as hex) to a small log file on disk.
//!
//! 2. At the start of `try_execute_immediately`, the caller registers the transaction digest with
//!    `register_executing_transaction`.  The returned `ExecutingTransactionGuard` clears the slot
//!    on drop, so the registration is always scoped to the execution of that single transaction.
//!
//!    The guard is deliberately `!Sync`.  `try_execute_immediately` is now a synchronous function,
//!    but this marker prevents any future change from accidentally moving the guard (and therefore
//!    the registration) across threads inside an async context, which would corrupt TLS.
//!
//!    The guard also carries the node's `db_path` and writes the crash log from its `Drop`
//!    implementation when it detects it is being dropped during a panic and TLS still holds the
//!    digest (meaning the process-level hook did not already write the log).  This makes the
//!    mechanism resilient to environments — such as simtests — where the panic hook chain may not
//!    be intact at the time of the crash.
//!
//! 3. On the next startup, `load_crashed_transactions` reads the log file and returns the set of
//!    digests that were active during a past crash.  The consensus handler uses this set to drop
//!    those transactions before they reach execution again.
//!
//! # Why TLS is correct here
//!
//! Rust's `panic` hook runs on the thread that panicked, so `with` on the TLS slot correctly sees
//! the digest that was registered by that thread's call to `try_execute_immediately`.  Threads that
//! were not executing a transaction at the time of the panic simply see an empty slot and are
//! ignored.

use std::{
    cell::Cell,
    collections::HashSet,
    fs::{self, OpenOptions},
    io::{BufRead, BufReader, Write},
    marker::PhantomData,
    path::{Path, PathBuf},
};

use sui_types::digests::TransactionDigest;
use tracing::{error, info, warn};

// ---------------------------------------------------------------------------
// Simtest-only: crash-cause signal
// ---------------------------------------------------------------------------
//
// In simtests there are two kinds of node kills:
//   1. crash-with-tx-logging: the executing transaction is the *cause* of the crash
//      (a deterministic bug). All validators crash for the same transaction → safe to drop.
//   2. All other fail points (batch-write-before, crash, etc.): the kill is random, not
//      caused by the transaction. Only one validator is killed. Writing the executing
//      transaction to the crash log would be wrong — other validators have already
//      committed it, so dropping it on restart causes a fork.
//
// In production builds this module does not exist and the guard writes on any panic,
// which is correct: production crashes are either deterministic (all validators hit the
// same bug) or hardware failures (which also affect all validators or are recovered by
// state sync).

#[cfg(msim)]
mod tx_crash_signal {
    use std::cell::Cell;
    thread_local! {
        /// Set by the crash-simulation fail point before triggering kill_current_node.
        /// Cleared and checked by the guard's Drop and panic hook.
        static ARMED: Cell<bool> = const { Cell::new(false) };
    }
    pub(super) fn arm() {
        ARMED.with(|c| c.set(true));
    }
    pub(super) fn disarm_and_check() -> bool {
        ARMED.with(|c| c.replace(false))
    }
}

/// Called by the crash-simulation fail point immediately before triggering
/// `kill_current_node`. Must be called on the same OS thread that holds the
/// `ExecutingTransactionGuard` so that the TLS flag is visible in the guard's Drop.
///
/// Without this signal, random node kills (from batch-write-before and similar fail
/// points) would record innocent transactions in the crash log, causing forks.
#[cfg(msim)]
pub fn arm_tx_crash_signal() {
    tx_crash_signal::arm();
}

// ---------------------------------------------------------------------------
// Simtest-only: deterministic crash decision
// ---------------------------------------------------------------------------

/// Return `true` if `digest` should be treated as a poison transaction in a simtest run.
///
/// Uses a process-global seed (initialised once from OS entropy via `OnceLock`) so that all
/// validators — regardless of which OS thread they run on — reach the same decision for a given
/// digest. This is necessary because the blocking thread-pool workers used by msim each have
/// distinct thread-local seeds, which would produce divergent crash decisions and checkpoint forks.
///
/// `prob` is the desired crash probability in the range `[0.0, 1.0]`.  Passing `0.002` means
/// approximately 0.2 % of user transactions are poisoned.
#[cfg(msim)]
pub fn should_poison_transaction(digest: &TransactionDigest, prob: f64) -> bool {
    use std::hash::{Hash, Hasher};

    static CRASH_SEED: std::sync::OnceLock<u64> = std::sync::OnceLock::new();
    let seed = *CRASH_SEED.get_or_init(|| {
        use rand::Rng;
        rand::thread_rng().r#gen::<u64>()
    });

    let mut hasher = std::collections::hash_map::DefaultHasher::new();
    seed.hash(&mut hasher);
    digest.hash(&mut hasher);
    let threshold = (prob.clamp(0.0, 1.0) * u64::MAX as f64) as u64;
    hasher.finish() < threshold
}

/// File name within the node's db_path where panicking-transaction digests are persisted.
const PANIC_TX_LOG_FILE: &str = "panic-tx.log";

// ---------------------------------------------------------------------------
// Thread-local slot
// ---------------------------------------------------------------------------

thread_local! {
    /// The digest of the transaction currently executing on this thread, if any.
    ///
    /// We use `Cell<Option<TransactionDigest>>` so that the panic hook (which takes `&PanicInfo`)
    /// can read it without needing `&mut` access or a `RefCell` borrow.
    static EXECUTING_TX: Cell<Option<TransactionDigest>> = const { Cell::new(None) };
}

// ---------------------------------------------------------------------------
// Scope guard
// ---------------------------------------------------------------------------

/// RAII guard that registers a transaction digest in thread-local storage for the duration of its
/// lifetime and removes it on drop.
///
/// The `PhantomData<*mut ()>` makes the guard `!Send + !Sync`, ensuring it is never moved to a
/// different thread (which would corrupt TLS) or shared across threads.
pub struct ExecutingTransactionGuard {
    /// The node's database directory, used to locate the crash log in case the process-level
    /// panic hook is not present at the time of the crash (e.g. in simtests).
    db_path: PathBuf,
    _not_send_sync: PhantomData<*mut ()>,
}

impl ExecutingTransactionGuard {
    fn new(digest: TransactionDigest, db_path: PathBuf) -> Self {
        EXECUTING_TX.with(|slot| slot.set(Some(digest)));
        Self {
            db_path,
            _not_send_sync: PhantomData,
        }
    }
}

impl Drop for ExecutingTransactionGuard {
    fn drop(&mut self) {
        // Read and clear TLS.  In the normal (non-panicking) path this is all we need to do.
        let digest = EXECUTING_TX.with(|slot| slot.get());
        EXECUTING_TX.with(|slot| slot.set(None));

        // If the process-level hook already wrote the log it will have cleared TLS, so `digest`
        // is None here and we are done.  If the hook was not in the chain (e.g. in msim, where
        // run_all_ready takes and replaces the hook chain at every iteration), TLS still has the
        // digest and we write the log now as a fallback.
        if let Some(digest) = digest {
            if std::thread::panicking() {
                // In simtests, only write when the crash-simulation fail point set the arm
                // signal. Random node kills from unrelated fail points (batch-write-before etc.)
                // also unwind the execution stack with thread::panicking() == true, but recording
                // the executing transaction in those cases is wrong: other validators have already
                // committed the transaction, so dropping it on restart causes a fork.
                #[cfg(msim)]
                if !tx_crash_signal::disarm_and_check() {
                    return;
                }
                let log_path = self.db_path.join(PANIC_TX_LOG_FILE);
                match append_digest_to_log(&log_path, digest) {
                    Ok(()) => eprintln!(
                        "[crash-recovery] Panic while executing transaction {digest}; \
                         recorded to {}",
                        log_path.display()
                    ),
                    Err(e) => eprintln!(
                        "[crash-recovery] Failed to write crashed transaction {digest} to \
                         {}: {e}",
                        log_path.display()
                    ),
                }
            }
        }
    }
}

/// Register `digest` as the transaction currently executing on this thread.
///
/// Returns a guard whose `Drop` implementation removes the registration. The registration lives
/// exactly as long as the guard, which should be kept in a local variable at the call site.
///
/// `db_path` is the node's database directory.  It is stored in the guard so that the crash log
/// can be written from the guard's `Drop` implementation if the process-level panic hook is not
/// in the hook chain at the time of the crash.
///
/// ```ignore
/// let _guard = register_executing_transaction(digest, db_path);
/// // ... execute the transaction ...
/// // guard is dropped here, clearing the TLS slot
/// ```
pub fn register_executing_transaction(
    digest: TransactionDigest,
    db_path: PathBuf,
) -> ExecutingTransactionGuard {
    ExecutingTransactionGuard::new(digest, db_path)
}

// ---------------------------------------------------------------------------
// Panic hook
// ---------------------------------------------------------------------------

/// Install a panic hook that appends the active transaction digest (if any) to the crash log.
///
/// This chains onto whatever panic hook is already installed (typically the tracing hook set up by
/// `telemetry-subscribers`), so existing behaviour is preserved.
///
/// `db_path` should be the node's base database directory; the log file is written at
/// `{db_path}/panic-tx.log`.
pub fn install_panic_hook(db_path: PathBuf) {
    // In simtests, all simulated nodes share the same OS process and panic hook chain. Each
    // node installs its own hook, prepending to the chain. When any panic fires, all hooks run
    // in reverse-install order. Without a node ID guard, the first hook in the chain would
    // consume the TLS digest and write it to the WRONG validator's log file, leaving the
    // actually-crashing validator with nothing in its log.
    //
    // Capturing the node ID at install time and gating on it at panic time ensures each hook
    // only claims panics that originated in its own node's execution context.
    //
    // NOTE: in simtests, msim's run_all_ready replaces the hook chain on every iteration, so
    // this hook is not guaranteed to fire.  The fallback is ExecutingTransactionGuard::drop,
    // which always has access to db_path and writes the log when dropped during a panic.
    #[cfg(msim)]
    let installing_node_id = sui_simulator::current_simnode_id();

    let prev_hook = std::panic::take_hook();
    std::panic::set_hook(Box::new(move |info| {
        // In simtests, skip this hook if the panic is firing in a different node's context.
        #[cfg(msim)]
        if sui_simulator::current_simnode_id() != installing_node_id {
            prev_hook(info);
            return;
        }

        // Write the crash log BEFORE calling the previous hook. Some previous hooks call
        // `process::exit` (e.g. the telemetry hook when crash_on_panic=true), and we must
        // not lose the digest in that race.
        //
        // We also clear the TLS slot after writing so that ExecutingTransactionGuard::drop
        // (which runs later during unwind) sees an empty slot and skips the duplicate write.
        let digest = EXECUTING_TX.with(|slot| slot.get());
        if let Some(digest) = digest {
            // In simtests, mirror the same arm-signal check used by the guard's Drop.
            #[cfg(msim)]
            if !tx_crash_signal::disarm_and_check() {
                prev_hook(info);
                return;
            }
            EXECUTING_TX.with(|slot| slot.set(None));
            let log_path = db_path.join(PANIC_TX_LOG_FILE);
            match append_digest_to_log(&log_path, digest) {
                Ok(()) => {
                    // Use eprintln rather than tracing: the subscriber may be in a broken
                    // state during a panic.
                    eprintln!(
                        "[crash-recovery] Panic while executing transaction {digest}; \
                         recorded to {}",
                        log_path.display()
                    );
                }
                Err(e) => {
                    eprintln!(
                        "[crash-recovery] Failed to write crashed transaction {digest} to \
                         {}: {e}",
                        log_path.display()
                    );
                }
            }
        }

        prev_hook(info);
    }));
}

fn append_digest_to_log(path: &Path, digest: TransactionDigest) -> std::io::Result<()> {
    let mut file = OpenOptions::new().create(true).append(true).open(path)?;
    writeln!(file, "{}", digest)?;
    file.flush()
}

// ---------------------------------------------------------------------------
// Startup: read crashed transactions
// ---------------------------------------------------------------------------

/// Read the panic log and return the set of transaction digests that were active during a past
/// crash.  Returns an empty set if the log file does not exist.
///
/// `db_path` should be the same path passed to `install_panic_hook`.
pub fn load_crashed_transactions(db_path: &Path) -> HashSet<TransactionDigest> {
    let log_path = db_path.join(PANIC_TX_LOG_FILE);
    match fs::File::open(&log_path) {
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => {
            return HashSet::new();
        }
        Err(e) => {
            error!(
                "Failed to open crash-recovery log {}: {e}",
                log_path.display()
            );
            return HashSet::new();
        }
        Ok(file) => {
            let mut digests = HashSet::new();
            for line in BufReader::new(file).lines() {
                match line {
                    Err(e) => {
                        warn!("Error reading crash-recovery log: {e}");
                    }
                    Ok(s) => {
                        let s = s.trim();
                        if s.is_empty() {
                            continue;
                        }
                        match s.parse::<TransactionDigest>() {
                            Ok(d) => {
                                info!(
                                    "Crash-recovery: will drop previously-crashing transaction {d}"
                                );
                                digests.insert(d);
                            }
                            Err(e) => {
                                warn!("Crash-recovery log contains unparseable digest {s:?}: {e}");
                            }
                        }
                    }
                }
            }
            digests
        }
    }
}
