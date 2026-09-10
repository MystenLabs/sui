// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

//! Reachability assertions whose "must be hit" expectation is decided at runtime from the
//! protocol config, rather than at compile time.
//!
//! `mysten_common::assert_reachable!` registers with Antithesis at compile time: every call
//! site linked into the binary is unconditionally expected to execute at least once during a
//! run. That is the wrong expectation for code behind a protocol feature flag. One sui-node
//! binary serves every chain configuration - Antithesis picks one per run by setting
//! `SUI_PROTOCOL_CONFIG_CHAIN_OVERRIDE` - so a flag that is on for one chain is off for
//! another, and a site that is correctly dark for the run's configuration is still reported as
//! "never reached". The same thing happens in reverse while a flag is rolling out: a path
//! guarded by `!flag` is dark on whichever chains already have the flag on.
//!
//! `assert_reachable_gated!` takes a predicate over `ProtocolConfig` and is catalogued only
//! once the node actually adopts a config that satisfies it:
//!
//! ```ignore
//! assert_reachable_gated!(
//!     "retry object withdraw later",
//!     |pc| !pc.check_object_funds_withdraw_in_execution()
//! );
//! ```
//!
//! `register_reachability_for_config` performs that registration and is called at every epoch
//! start. Registration is cumulative across epochs, so an upgrade test that starts below a
//! flag's enabling version and upgrades through it requires both the old and the new path -
//! which is correct, since both really do execute during the run.

use crate::ProtocolConfig;
use antithesis_sdk::assert::{AssertType, assert_raw};
use antithesis_sdk::linkme::distributed_slice;
use serde_json::json;
use std::sync::atomic::{AtomicU8, Ordering};

/// A catalog entry has been emitted for this point.
const CATALOGUED: u8 = 1 << 0;
/// The emitted catalog entry has `must_hit: true`.
const REQUIRED: u8 = 1 << 1;
/// A `hit` has been emitted for this point.
const HIT: u8 = 1 << 2;

/// A reachability assertion that is registered with Antithesis at runtime.
///
/// Constructed by [`assert_reachable_gated!`]; there is no reason to name this type directly.
pub struct GatedReachabilityPoint {
    pub message: &'static str,
    pub class: &'static str,
    /// Resolves the enclosing function's path. A fn pointer rather than a `&'static str`
    /// because the `type_name` trick it uses has to be expanded at the call site.
    pub function: fn() -> &'static str,
    pub file: &'static str,
    pub line: u32,
    pub column: u32,
    /// Whether the point is expected to be reachable under a given protocol config.
    pub expected_reachable: fn(&ProtocolConfig) -> bool,
    pub state: AtomicU8,
}

/// Every [`assert_reachable_gated!`] site linked into the binary.
#[distributed_slice]
#[linkme(crate = antithesis_sdk::linkme)]
pub static GATED_REACHABILITY_CATALOG: [GatedReachabilityPoint];

/// Forces a non-capturing closure at a call site to the predicate signature, so that
/// `|pc| ...` can be written without naming `ProtocolConfig`.
pub const fn as_predicate(f: fn(&ProtocolConfig) -> bool) -> fn(&ProtocolConfig) -> bool {
    f
}

impl GatedReachabilityPoint {
    /// Whether Antithesis has been told this point must be hit.
    pub fn is_required(&self) -> bool {
        self.state.load(Ordering::Relaxed) & REQUIRED != 0
    }

    /// Emits a catalog entry if one is still owed, and reports whether the point is required.
    ///
    /// `required` only ever upgrades: once a config has made the point live, a later config
    /// that does not is not a reason to stop expecting the hit, because the earlier config
    /// really did run.
    fn ensure_catalogued(&self, required: bool) -> bool {
        let wanted = CATALOGUED | if required { REQUIRED } else { 0 };
        let previous = self.state.fetch_or(wanted, Ordering::Relaxed);
        let required = required || previous & REQUIRED != 0;
        if !previous & wanted != 0 {
            self.emit(false, required);
        }
        required
    }

    /// Records that control flow reached this point. Called by [`assert_reachable_gated!`].
    pub fn reached(&self) {
        // Only the first hit is reported, so keep the steady state to a single relaxed load:
        // some of these sit on per-transaction paths.
        if self.state.load(Ordering::Relaxed) & HIT != 0 {
            return;
        }
        // A point can be reached without having been catalogued: either the predicate is
        // wrong, or this is a binary that never adopts a protocol config (the stress client,
        // for instance). Catalog it as optional so that the hit is still well formed - the
        // SDK requires a catalog entry for every assertion issued through `assert_raw`.
        let required = self.ensure_catalogued(false);
        if self.state.fetch_or(HIT, Ordering::Relaxed) & HIT == 0 {
            self.emit(true, required);
        }
    }

    fn emit(&self, hit: bool, must_hit: bool) {
        // Mirror what the sdk's own macros put on the wire: catalog entries carry
        // `condition: false` and an empty payload, hits carry `condition: true`.
        let details = if hit { json!({}) } else { json!(null) };
        assert_raw(
            hit,                          // condition
            self.message.to_owned(),      // message
            &details,                     // details
            self.class.to_owned(),        // class
            (self.function)().to_owned(), // function
            self.file.to_owned(),         // file
            self.line,                    // begin_line
            self.column,                  // begin_column
            hit,                          // hit
            must_hit,                     // must_hit
            AssertType::Reachability,     // assert_type
            "Reachable".to_owned(),       // display_type
            self.message.to_owned(),      // id
        );
    }
}

/// Catalogs every gated point that `config` makes live.
///
/// Call this each time the node adopts a protocol config, including at startup. It is
/// idempotent, so repeated calls with the same config emit nothing after the first.
pub fn register_reachability_for_config(config: &ProtocolConfig) {
    // Calling in to the antithesis sdk breaks determinism in simtests (on linux only).
    if cfg!(msim) {
        return;
    }
    for point in GATED_REACHABILITY_CATALOG.iter() {
        if (point.expected_reachable)(config) {
            point.ensure_catalogued(true);
        }
    }
}

/// Like `mysten_common::assert_reachable!`, but only expected to be reached under protocol
/// configs satisfying `$expected_reachable`. See the module docs.
///
/// The predicate must be a non-capturing closure over `&ProtocolConfig`.
#[macro_export]
macro_rules! assert_reachable_gated {
    ($message:literal, $expected_reachable:expr) => {{
        // `_f`'s `type_name` is the enclosing function's path with `::_f` appended, which is
        // how the antithesis sdk recovers a function name. It has to be defined here, in the
        // caller's body, rather than inside a helper.
        fn _f() {}
        fn __function_name() -> &'static str {
            fn type_name_of<T>(_: T) -> &'static str {
                ::std::any::type_name::<T>()
            }
            let name = type_name_of(_f);
            &name[..name.len() - "::_f".len()]
        }

        #[$crate::linkme::distributed_slice($crate::reachability::GATED_REACHABILITY_CATALOG)]
        #[linkme(crate = $crate::linkme)]
        static POINT: $crate::reachability::GatedReachabilityPoint =
            $crate::reachability::GatedReachabilityPoint {
                message: $message,
                class: ::std::module_path!(),
                function: __function_name,
                file: ::std::file!(),
                line: ::std::line!(),
                column: ::std::column!(),
                expected_reachable: $crate::reachability::as_predicate($expected_reachable),
                state: ::std::sync::atomic::AtomicU8::new(0),
            };

        // calling in to antithesis sdk breaks determinisim in simtests (on linux only)
        if !cfg!(msim) {
            POINT.reached();
        } else {
            $crate::assert_reachable_simtest!($message);
        }
    }};
}

// Registration is skipped under msim, so these cannot run there.
#[cfg(all(test, not(msim)))]
mod tests {
    use super::*;
    use crate::{Chain, ProtocolVersion};

    fn find(message: &str) -> &'static GatedReachabilityPoint {
        GATED_REACHABILITY_CATALOG
            .iter()
            .find(|point| point.message == message)
            .expect("point should be linked into the catalog")
    }

    fn always_live() {
        assert_reachable_gated!("test point: always live", |_| true);
    }

    fn never_live() {
        assert_reachable_gated!("test point: never live", |_| false);
    }

    #[test]
    fn registers_only_points_the_config_makes_live() {
        // Force codegen of the enclosing functions so their catalog entries are linked in.
        let _ = (always_live as fn(), never_live as fn());

        let config = ProtocolConfig::get_for_version(ProtocolVersion::MAX, Chain::Unknown);
        register_reachability_for_config(&config);

        assert!(find("test point: always live").is_required());
        assert!(!find("test point: never live").is_required());
    }
}
