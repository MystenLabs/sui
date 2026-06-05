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

#[cfg(msim)]
use std::{cell::RefCell, collections::HashMap};

use sui_types::digests::TransactionDigest;
use tracing::{error, info, warn};

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

    /// In simtests, msim's `run_all_ready` calls `take_hook()` at the start of every iteration,
    /// which wipes the process-level panic hook chain.  By the time a validator processes a
    /// transaction that was previously seen to crash the node, the hooks installed during startup
    /// are gone, so the "crash-simulation" panic never triggers log-writing.
    ///
    /// As a fallback, we store each simulated node's db_path in this TLS map.  Thread-local
    /// storage is unaffected by msim's hook management.  `ExecutingTransactionGuard::drop` consults
    /// this map when it detects that it is being dropped during a panic and TLS still holds a
    /// digest (meaning the process-level hook did not fire).
    #[cfg(msim)]
    static SIM_NODE_DB_PATHS: RefCell<HashMap<msim::task::NodeId, PathBuf>> =
        RefCell::new(HashMap::new());
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
    _not_send_sync: PhantomData<*mut ()>,
}

impl ExecutingTransactionGuard {
    fn new(digest: TransactionDigest) -> Self {
        EXECUTING_TX.with(|slot| slot.set(Some(digest)));
        Self {
            _not_send_sync: PhantomData,
        }
    }
}

impl Drop for ExecutingTransactionGuard {
    fn drop(&mut self) {
        // In msim, the process-level panic hook is wiped by msim's `run_all_ready` at the start
        // of each iteration, so it may not fire for the "crash-simulation" panic.  If we are
        // being dropped during a panic and TLS still holds a digest (meaning the hook did not
        // clear it), write the crash log now using the per-node path registered in
        // SIM_NODE_DB_PATHS (TLS, unaffected by msim's hook management).
        #[cfg(msim)]
        if std::thread::panicking() {
            if let Some(digest) = EXECUTING_TX.with(|slot| slot.get()) {
                EXECUTING_TX.with(|slot| slot.set(None));
                let node_id = sui_simulator::current_simnode_id();
                let log_path = SIM_NODE_DB_PATHS.with(|map| {
                    map.borrow()
                        .get(&node_id)
                        .map(|p| p.join(PANIC_TX_LOG_FILE))
                });
                if let Some(log_path) = log_path {
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

        EXECUTING_TX.with(|slot| slot.set(None));
    }
}

/// Register `digest` as the transaction currently executing on this thread.
///
/// Returns a guard whose `Drop` implementation removes the registration. The registration lives
/// exactly as long as the guard, which should be kept in a local variable at the call site.
///
/// ```ignore
/// let _guard = register_executing_transaction(digest);
/// // ... execute the transaction ...
/// // guard is dropped here, clearing the TLS slot
/// ```
pub fn register_executing_transaction(digest: TransactionDigest) -> ExecutingTransactionGuard {
    ExecutingTransactionGuard::new(digest)
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
    // IMPORTANT: the process-level hook is only a best-effort mechanism in simtests.  msim's
    // `run_all_ready` calls `take_hook()` at the start of each iteration, wiping the entire hook
    // chain.  Hooks installed during one iteration are gone in subsequent iterations.  As a
    // result the process-level hook may not fire when the crash happens.  The real safety net is
    // `ExecutingTransactionGuard::drop`, which consults `SIM_NODE_DB_PATHS` (TLS, unaffected by
    // msim) when it detects it is being dropped during a panic.
    #[cfg(msim)]
    let installing_node_id = {
        let id = sui_simulator::current_simnode_id();
        SIM_NODE_DB_PATHS.with(|map| {
            map.borrow_mut().insert(id, db_path.clone());
        });
        id
    };

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
        // not lose the digest in that race. In simtests this hook is triggered via
        // `catch_unwind` from the fail point; the previous hook just logs and returns.
        //
        // We also clear the TLS slot after writing so that a second invocation of this hook
        // (e.g. from `kill_current_node`'s PanicWrapper panic) is a no-op.
        let digest = EXECUTING_TX.with(|slot| slot.get());
        if let Some(digest) = digest {
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
