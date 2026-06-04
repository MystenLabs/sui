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

use sui_types::digests::TransactionDigest;
use tracing::{error, info, warn};

/// File name within the node's db_path where panicking-transaction digests are persisted.
pub const PANIC_TX_LOG_FILE: &str = "panic-tx.log";

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
    let prev_hook = std::panic::take_hook();
    std::panic::set_hook(Box::new(move |info| {
        // Run the previous hook first so tracing/logging happens before we do file I/O.
        prev_hook(info);

        let digest = EXECUTING_TX.with(|slot| slot.get());
        if let Some(digest) = digest {
            let log_path = db_path.join(PANIC_TX_LOG_FILE);
            match append_digest_to_log(&log_path, digest) {
                Ok(()) => {
                    // Use eprintln rather than tracing here: the subscriber may be in a broken
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
    }));
}

pub fn append_digest_to_log(path: &Path, digest: TransactionDigest) -> std::io::Result<()> {
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
