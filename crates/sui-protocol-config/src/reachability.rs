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
//! `assert_reachable_gated!` takes a predicate over `ProtocolConfig` and registers a reachability
//! expectation when the node adopts a config that satisfies it:
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
//! flag's enabling version and upgrades through it requires both the old and the new path,
//! since both configurations were adopted. A point reached before registration is also
//! catalogued as required: the observed hit already satisfies that expectation.

use crate::ProtocolConfig;
use antithesis_sdk::assert::{AssertType, assert_raw};
use antithesis_sdk::linkme::distributed_slice;
use serde_json::json;
use std::sync::{
    Once,
    atomic::{AtomicBool, Ordering},
};

/// A reachability assertion that is registered with Antithesis at runtime.
///
/// Constructed by [`crate::assert_reachable_gated!`]; there is no reason to name this type directly.
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
    pub catalogued: Once,
    pub hit: AtomicBool,
}

/// Every [`crate::assert_reachable_gated!`] site linked into the binary.
#[distributed_slice]
#[linkme(crate = antithesis_sdk::linkme)]
pub static GATED_REACHABILITY_CATALOG: [GatedReachabilityPoint];

/// Forces a non-capturing closure at a call site to the predicate signature, so that
/// `|pc| ...` can be written without naming `ProtocolConfig`.
pub const fn as_predicate(f: fn(&ProtocolConfig) -> bool) -> fn(&ProtocolConfig) -> bool {
    f
}

impl GatedReachabilityPoint {
    fn ensure_catalogued(&self) {
        // A concurrent first hit must wait until the declaration has been emitted.
        self.catalogued.call_once(|| self.emit(false));
    }

    /// Records that control flow reached this point. Called by [`crate::assert_reachable_gated!`].
    pub fn reached(&self) {
        // Only the first hit is reported, so keep the steady state to a single relaxed load:
        // some of these sit on per-transaction paths.
        if self.hit.load(Ordering::Relaxed) {
            return;
        }
        // A binary without an epoch store can reach a point before registration. Requiring
        // an already-observed point is safe and keeps its SDK declaration stable.
        self.ensure_catalogued();
        if !self.hit.swap(true, Ordering::Relaxed) {
            self.emit(true);
        }
    }

    fn emit(&self, hit: bool) {
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
            true,                         // must_hit
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
            point.ensure_catalogued();
        }
    }
}

/// Like `mysten_common::assert_reachable!`, but only expected to be reached under protocol
/// configs satisfying `$expected_reachable`. See the module docs.
///
/// The predicate must be a non-capturing closure over `&ProtocolConfig` that does not panic
/// for any supported configuration; it runs during epoch-store construction.
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
                catalogued: ::std::sync::Once::new(),
                hit: ::std::sync::atomic::AtomicBool::new(false),
            };

        // calling in to antithesis sdk breaks determinism in simtests (on linux only)
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
    use std::{path::Path, process::Command};

    /// A config with the gating flag forced on or off.
    ///
    /// Pinning real protocol versions would couple this test to one flag's rollout schedule,
    /// and would break once MIN_PROTOCOL_VERSION advances past them. The code under test only
    /// ever sees a `ProtocolConfig`.
    fn config_with_flag(enabled: bool) -> ProtocolConfig {
        let mut config = ProtocolConfig::get_for_max_version_UNSAFE();
        config.set_check_object_funds_withdraw_in_execution_for_testing(enabled);
        config
    }

    fn early_hit() {
        assert_reachable_gated!("gated reachability test: early", |pc| pc
            .check_object_funds_withdraw_in_execution());
    }

    fn legacy() {
        assert_reachable_gated!("gated reachability test: legacy", |pc| !pc
            .check_object_funds_withdraw_in_execution());
    }

    fn upgraded() {
        assert_reachable_gated!("gated reachability test: upgraded", |pc| pc
            .check_object_funds_withdraw_in_execution());
    }

    fn disabled() {
        assert_reachable_gated!("gated reachability test: disabled", |_| false);
    }

    fn assert_output(path: &Path, expected: &[(&str, bool)]) {
        use mysten_common::ZipDebugEqIteratorExt as _;

        let output = std::fs::read_to_string(path).unwrap();
        let assertions: Vec<_> = output
            .lines()
            .map(|line| serde_json::from_str::<serde_json::Value>(line).unwrap())
            .filter_map(|mut event| {
                let assertion = event.get_mut("antithesis_assert")?;
                assertion["id"]
                    .as_str()?
                    .starts_with("gated reachability test:")
                    .then(|| assertion.take())
            })
            .collect();
        assert_eq!(assertions.len(), expected.len(), "{assertions:#?}");
        for (assertion, (id, hit)) in assertions.iter().zip_debug_eq(expected) {
            assert_eq!(assertion["id"], *id);
            assert_eq!(assertion["hit"], *hit);
            assert_eq!(assertion["condition"], *hit);
            assert_eq!(assertion["must_hit"], true);
            assert_eq!(assertion["assert_type"], "reachability");
            assert_eq!(assertion["display_type"], "Reachable");
        }
    }

    #[test]
    fn emits_stable_reachability_across_registration_and_hits() {
        let expected = [
            ("gated reachability test: early", false),
            ("gated reachability test: early", true),
            ("gated reachability test: legacy", false),
            ("gated reachability test: legacy", true),
            ("gated reachability test: upgraded", false),
            ("gated reachability test: upgraded", true),
        ];
        const CHILD: &str = "SUI_REACHABILITY_TEST_CHILD";
        if std::env::var_os(CHILD).is_none() {
            // The SDK caches both its output destination and its assertion tracker globally.
            // A subprocess isolates them without changing the test runner's environment.
            let dir = tempfile::tempdir().unwrap();
            let path = dir.path().join("sdk.jsonl");
            let output = Command::new(std::env::current_exe().unwrap())
                .args([
                    "--exact",
                    "reachability::tests::emits_stable_reachability_across_registration_and_hits",
                    "--nocapture",
                ])
                .env(CHILD, "1")
                .env("ANTITHESIS_SDK_LOCAL_OUTPUT", &path)
                .output()
                .unwrap();
            assert!(
                output.status.success(),
                "child failed:\nstdout: {}\nstderr: {}",
                String::from_utf8_lossy(&output.stdout),
                String::from_utf8_lossy(&output.stderr),
            );
            assert_output(&path, &expected);
            return;
        }

        std::hint::black_box(disabled as fn());
        let path = std::env::var_os("ANTITHESIS_SDK_LOCAL_OUTPUT").unwrap();
        let path = Path::new(&path);

        early_hit();
        assert_output(path, &expected[..2]);

        let old = config_with_flag(false);
        register_reachability_for_config(&old);
        register_reachability_for_config(&old);
        assert_output(path, &expected[..3]);

        legacy();
        legacy();
        assert_output(path, &expected[..4]);

        let new = config_with_flag(true);
        register_reachability_for_config(&new);
        register_reachability_for_config(&new);
        assert_output(path, &expected[..5]);

        upgraded();
        upgraded();
        early_hit();
        register_reachability_for_config(&old);
        assert_output(path, &expected);
    }
}
