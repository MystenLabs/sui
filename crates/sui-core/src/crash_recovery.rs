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
//!
//! # Drop guard vs. panic hook
//!
//! The drop guard does NOT write the crash log.  In production builds, `panic = "abort"` means
//! drop is never called during a panic, so only the hook can do it.  In simtest builds the hook
//! also runs synchronously (it fires inside the `catch_unwind` scope that simulates the crash),
//! so the hook is always the right place to write.

use std::{
    cell::Cell,
    collections::{HashMap, HashSet},
    fs::{self, OpenOptions},
    io::{BufRead, BufReader, Write},
    marker::PhantomData,
    path::{Path, PathBuf},
    sync::{Mutex, OnceLock},
};

use sui_types::{base_types::AuthorityName, digests::TransactionDigest};
use tracing::{error, info, warn};

macro_rules! git_revision {
    () => {
        match option_env!("GIT_REVISION") {
            Some(r) => r,
            None => "unknown",
        }
    };
}

const GIT_REVISION: &str = git_revision!();

static FILE_WRITE_LOCK: Mutex<()> = Mutex::new(());

// ---------------------------------------------------------------------------
// Simtest-only: catch_unwind panic marker
// ---------------------------------------------------------------------------

/// Panic message used by the crash-simulation fail point in simtests.
///
/// The panic hook identifies intentional crash-simulation panics by checking for this exact
/// payload.  `kill_current_node` panics with a private `PanicWrapper` type, so plain `&str`
/// downcasting cleanly discriminates the two kinds of panic.
#[cfg(msim)]
pub const CRASH_SIM_PANIC_MSG: &str = "crash-simulation";

// ---------------------------------------------------------------------------
// Deterministic crash decision (available in all test configurations)
// ---------------------------------------------------------------------------

static CRASH_RECOVERY_PROBABILITY_1E6: std::sync::atomic::AtomicU32 =
    std::sync::atomic::AtomicU32::new(0);

/// Set the fraction of user transactions that should be treated as poison.
/// A poison transaction triggers a crash-recovery test cycle: the node panics
/// while executing it, writes the digest to the crash log, and on restart drops
/// the transaction so it cannot crash the node again.
///
/// Has no effect in release builds because `should_poison_transaction` is only
/// called when `in_test_configuration()` is true.
pub fn set_crash_recovery_probability(prob: f64) {
    CRASH_RECOVERY_PROBABILITY_1E6.store(
        (prob.clamp(0.0, 1.0) * 1_000_000.0) as u32,
        std::sync::atomic::Ordering::Relaxed,
    );
}

/// Returns the crash probability: the explicitly set value if any, otherwise a
/// default of 0.2 % in Antithesis (where explicit setup is not always possible),
/// or `None` everywhere else.
pub fn crash_recovery_probability() -> Option<f64> {
    let v = CRASH_RECOVERY_PROBABILITY_1E6.load(std::sync::atomic::Ordering::Relaxed);
    if v != 0 {
        Some(v as f64 / 1_000_000.0)
    } else if mysten_common::in_antithesis() {
        Some(0.002)
    } else {
        None
    }
}

/// Return `true` if `digest` should be treated as a poison transaction.
///
/// Uses a process-global seed (initialised once from OS entropy via `OnceLock`) so that all
/// Returns `false` if no crash probability has been set via `set_crash_recovery_probability`.
///
/// In simtests a process-global random seed is mixed in so that different test seeds exercise
/// different transactions.  Outside simtests (Antithesis, debug builds) the decision is purely
/// content-addressed so that independent validator processes always agree on which transactions
/// are poison.
pub fn should_poison_transaction(digest: &TransactionDigest) -> bool {
    let Some(prob) = crash_recovery_probability() else {
        return false;
    };

    #[cfg(msim)]
    {
        use std::hash::{Hash, Hasher};
        static CRASH_SEED: OnceLock<u64> = OnceLock::new();
        let seed = *CRASH_SEED.get_or_init(|| {
            use rand::Rng;
            rand::thread_rng().r#gen::<u64>()
        });
        let mut hasher = std::collections::hash_map::DefaultHasher::new();
        seed.hash(&mut hasher);
        digest.hash(&mut hasher);
        let threshold = (prob.clamp(0.0, 1.0) * u64::MAX as f64) as u64;
        return hasher.finish() < threshold;
    }

    #[cfg(not(msim))]
    mysten_common::random::content_addressed_probability(digest.as_ref(), prob as f32)
}

/// Trigger a crash-recovery cycle for `digest` if it is in the poison set.
///
/// In simtests: uses a fail point so the test harness controls when crashes fire; the
/// panic is caught and re-raised via `kill_current_node` so the sim can restart the node.
/// In other test configurations (debug builds, Antithesis): panics directly; the panic
/// hook writes the digest to the crash log and the process terminates.
/// In release builds: no-op (the `not(msim)` branch is compiled out by
/// `in_test_configuration()` always returning false, and the msim branch is not compiled).
pub fn maybe_crash_for_testing(digest: &TransactionDigest) {
    #[cfg(msim)]
    {
        sui_macros::fail_point_if!("crash-with-tx-logging", || {
            if should_poison_transaction(digest) {
                // Use catch_unwind to trigger the panic hook without crashing the test process.
                let _ = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
                    std::panic::panic_any(CRASH_SIM_PANIC_MSG);
                }));
                sui_simulator::task::kill_current_node(Some(std::time::Duration::from_millis(100)));
            }
        });
    }
    #[cfg(not(msim))]
    if mysten_common::in_test_configuration() && should_poison_transaction(digest) {
        panic!("crash-recovery: transaction {digest}");
    }
}

/// Mine a non-poison digest by adding an unused pure argument to the PTB.
///
/// If no crash probability is set, this is a no-op. Otherwise, iterates until the signed
/// transaction's digest is not in the poison set.
#[cfg(msim)]
pub fn mine_non_poison_transaction(
    tx_data: &mut sui_types::transaction::TransactionData,
    signer: &dyn sui_types::crypto::Signer<sui_types::crypto::Signature>,
) {
    use sui_types::transaction::{CallArg, TransactionDataAPI as _, TransactionKind};

    if crash_recovery_probability().is_none() {
        return;
    }

    let mut nonce: u64 = 0;
    loop {
        let tx = sui_types::transaction::Transaction::from_data_and_signer(
            tx_data.clone(),
            vec![signer],
        );
        if !should_poison_transaction(tx.digest()) {
            break;
        }
        nonce += 1;
        if let TransactionKind::ProgrammableTransaction(pt) = tx_data.kind_mut() {
            if nonce == 1 {
                pt.inputs
                    .push(CallArg::Pure(bcs::to_bytes(&nonce).unwrap()));
            } else {
                *pt.inputs.last_mut().unwrap() = CallArg::Pure(bcs::to_bytes(&nonce).unwrap());
            }
        }
    }
}

/// File name within the node's db_path where panicking-transaction digests are persisted.
const PANIC_TX_LOG_FILE: &str = "panic-tx.log";

// ---------------------------------------------------------------------------
// Global registry: AuthorityName → db_path
// ---------------------------------------------------------------------------
//
// The panic hook needs to write to a specific log file. Rather than carrying a PathBuf through
// TLS (which would require heap allocation on every transaction), we keep a small process-wide
// map from AuthorityName to db_path that is populated once at startup by `install_panic_hook`.

static NODE_DB_PATHS: OnceLock<Mutex<HashMap<AuthorityName, PathBuf>>> = OnceLock::new();

fn node_db_paths() -> &'static Mutex<HashMap<AuthorityName, PathBuf>> {
    NODE_DB_PATHS.get_or_init(|| Mutex::new(HashMap::new()))
}

fn get_db_path(authority_name: &AuthorityName) -> Option<PathBuf> {
    node_db_paths().lock().ok()?.get(authority_name).cloned()
}

// ---------------------------------------------------------------------------
// Thread-local slot
// ---------------------------------------------------------------------------

thread_local! {
    /// The authority and digest of the transaction currently executing on this thread, if any.
    ///
    /// We use `Cell<Option<(AuthorityName, TransactionDigest)>>` so that the panic hook (which
    /// takes `&PanicInfo`) can read it without needing `&mut` access or a `RefCell` borrow.
    /// Both `AuthorityName` and `TransactionDigest` are `Copy`, so `Cell` is appropriate.
    ///
    /// Storing the authority name alongside the digest lets each validator's panic hook
    /// identify — purely from TLS — whether it is responsible for writing the crash log,
    /// without relying on any external node-identity mechanism.
    static EXECUTING_TX: Cell<Option<(AuthorityName, TransactionDigest)>> =
        const { Cell::new(None) };
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
    fn new(digest: TransactionDigest, authority_name: AuthorityName) -> Self {
        EXECUTING_TX.with(|slot| slot.set(Some((authority_name, digest))));
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
/// `authority_name` identifies the node. It is used to look up the crash-log path from the
/// registry populated by `install_panic_hook`, and to ensure each validator's panic hook writes
/// only to its own log.
///
/// ```ignore
/// let _guard = register_executing_transaction(digest, authority_name);
/// // ... execute the transaction ...
/// // guard is dropped here, clearing the TLS slot
/// ```
pub fn register_executing_transaction(
    digest: TransactionDigest,
    authority_name: AuthorityName,
) -> ExecutingTransactionGuard {
    ExecutingTransactionGuard::new(digest, authority_name)
}

// ---------------------------------------------------------------------------
// Panic hook
// ---------------------------------------------------------------------------

/// Install a panic hook that appends the active transaction digest (if any) to the crash log.
///
/// This chains onto whatever panic hook is already installed (typically the tracing hook set up by
/// `telemetry-subscribers`), so existing behaviour is preserved.
///
/// `authority_name` is this node's public key identity; it is used as the key in
/// `NODE_DB_PATHS` and to gate the hook so that each validator only claims panics that
/// originated in its own execution context.
///
/// `db_path` is the node's base database directory; the log file is written at
/// `{db_path}/panic-tx.log`.
pub fn install_panic_hook(authority_name: AuthorityName, db_path: PathBuf) {
    // Register the authority → db_path mapping so the hook can locate the log file.
    node_db_paths()
        .lock()
        .expect("NODE_DB_PATHS poisoned")
        .insert(authority_name, db_path);

    // In simtests, all simulated nodes share the same OS process and panic hook chain. Each
    // node installs its own hook, prepending to the chain. When any panic fires, all hooks run
    // in reverse-install order.
    //
    // Each hook captures `authority_name` at install time and compares it to the authority name
    // stored in TLS at panic time. Because `register_executing_transaction` writes the executing
    // node's authority name into TLS, each hook correctly claims only panics that originated in
    // its own node's execution context.
    //
    // In simtests, two distinct panic kinds flow through the hook chain:
    //   1. `panic!("{}", CRASH_SIM_PANIC_MSG)` inside a `catch_unwind`: the intended crash
    //      simulation. The payload is a plain `&str` and we should write the crash log.
    //   2. `kill_current_node(...)`: simulates a node going down. It panics with a private
    //      `PanicWrapper` struct, so `downcast_ref::<&str>()` returns `None`. We should NOT
    //      write the crash log for these — the node is being killed for an unrelated reason and
    //      the other validators have already committed the transaction.
    //
    // In non-sim builds any panic is a genuine crash and we always write.
    let prev_hook = std::panic::take_hook();
    std::panic::set_hook(Box::new(move |info| {
        // Write the crash log BEFORE calling the previous hook. Some previous hooks call
        // `process::exit` (e.g. the telemetry hook when crash_on_panic=true), and we must
        // not lose the digest in that race.
        //
        // We also clear the TLS slot after writing so that (in unwind builds) any drop impls
        // that run later see an empty slot.
        let entry = EXECUTING_TX.with(|slot| slot.get());
        if let Some((tls_authority, digest)) = entry {
            if tls_authority == authority_name {
                // In simtests, only write for intentional crash-simulation panics. Random node
                // kills from unrelated fail points panic with a PanicWrapper (not a &str), so
                // this check naturally excludes them.
                #[cfg(msim)]
                if info.payload().downcast_ref::<&str>().copied() != Some(CRASH_SIM_PANIC_MSG) {
                    prev_hook(info);
                    return;
                }

                EXECUTING_TX.with(|slot| slot.set(None));
                if let Some(db_path) = get_db_path(&authority_name) {
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
            }
        }

        prev_hook(info);
    }));
}

fn append_digest_to_log(path: &Path, digest: TransactionDigest) -> std::io::Result<()> {
    let _guard = FILE_WRITE_LOCK.lock().unwrap_or_else(|e| e.into_inner());
    let mut file = OpenOptions::new().create(true).append(true).open(path)?;
    writeln!(file, "{} {}", GIT_REVISION, digest)?;
    file.flush()
}

// ---------------------------------------------------------------------------
// Startup: read crashed transactions
// ---------------------------------------------------------------------------

/// Read the panic log and return the set of transaction digests that were active during a past
/// crash.  Returns an empty set if the log file does not exist.
///
/// Lines whose git revision does not match the current binary are skipped and removed from the
/// log (the file is rewritten with only matching-revision lines).  This prevents stale entries
/// from a different binary version from poisoning new runs.
///
/// `db_path` should be the same path passed to `install_panic_hook`.
pub fn load_crashed_transactions(db_path: &Path) -> HashSet<TransactionDigest> {
    const LINE_LIMIT: usize = 1024 * 1024;

    let log_path = db_path.join(PANIC_TX_LOG_FILE);
    let file = match fs::File::open(&log_path) {
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
        Ok(f) => f,
    };

    let mut digests = HashSet::new();
    let mut kept_lines: Vec<String> = Vec::new();
    let mut any_skipped = false;

    for line in BufReader::new(file).lines().take(LINE_LIMIT) {
        match line {
            Err(e) => {
                warn!("Error reading crash-recovery log: {e}");
            }
            Ok(s) => {
                let s = s.trim().to_owned();
                if s.is_empty() {
                    continue;
                }
                // Expected format: "<git_revision> <tx_digest>".
                // Old single-token lines (bare digest) are treated as non-matching.
                let mut parts = s.splitn(2, ' ');
                let (rev, digest_str) = match (parts.next(), parts.next()) {
                    (Some(r), Some(d)) => (r, d),
                    _ => {
                        warn!("Crash-recovery log contains unrecognised entry {s:?}; skipping");
                        any_skipped = true;
                        continue;
                    }
                };

                if rev != GIT_REVISION {
                    info!("Crash-recovery: skipping entry from different binary revision {rev:?}");
                    any_skipped = true;
                    continue;
                }

                match digest_str.parse::<TransactionDigest>() {
                    Ok(d) => {
                        info!("Crash-recovery: will drop previously-crashing transaction {d}");
                        digests.insert(d);
                        kept_lines.push(s);
                    }
                    Err(e) => {
                        warn!("Crash-recovery log contains unparseable digest {digest_str:?}: {e}");
                        any_skipped = true;
                    }
                }
            }
        }
    }

    if any_skipped {
        let _guard = FILE_WRITE_LOCK.lock().unwrap_or_else(|e| e.into_inner());
        match OpenOptions::new()
            .write(true)
            .truncate(true)
            .create(true)
            .open(&log_path)
        {
            Ok(mut f) => {
                for line in &kept_lines {
                    if let Err(e) = writeln!(f, "{line}") {
                        warn!("Failed to rewrite crash-recovery log: {e}");
                        break;
                    }
                }
            }
            Err(e) => warn!(
                "Failed to rewrite crash-recovery log {}: {e}",
                log_path.display()
            ),
        }
    }

    digests
}
