// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

//! Shared polling/retry/dedup/metrics/cap/consensus-submission infrastructure used by both the
//! zkLogin OAuth JWK updater and the GCP Confidential Space JWK updater in [`crate`].
//!
//! Source-specific behavior -- how to fetch and parse a response, and what makes a given
//! `(JwkId, JWK)` valid for that source -- is intentionally *not* here: callers inject it via
//! plain closures. Everything else (immediate fetch, retry-on-error, per-key active-epoch
//! filtering, per-task de-duplication, the [`MAX_JWK_KEYS_PER_FETCH`] cap, and individual
//! consensus submission) lives in this module so the two updaters can't silently drift apart.

use fastcrypto_zkp::bn254::zk_login::{JWK, JwkId};
use std::collections::HashSet;
use std::future::Future;
use std::sync::Arc;
use std::time::Duration;
use sui_core::authority::authority_per_epoch_store::AuthorityPerEpochStore;
use sui_core::consensus_adapter::ConsensusAdapter;
use sui_types::base_types::AuthorityName;
use sui_types::error::SuiResult;
use sui_types::messages_consensus::ConsensusTransaction;
use tap::tap::TapFallible;
use tracing::warn;

use crate::metrics::SuiNodeMetrics;

// Logs at debug level in test configuration, info level otherwise. JWK logs cause significant
// volume in tests, but are insignificant in prod, so we keep them at info. Kept as a local copy
// of `crate::jwk_log` (rather than sharing one macro across the module boundary) since
// `macro_rules!` items are only visible after their point of definition.
macro_rules! jwk_log {
    ($($arg:tt)+) => {
        if mysten_common::in_test_configuration() {
            tracing::debug!($($arg)+);
        } else {
            tracing::info!($($arg)+);
        }
    };
}

/// Maximum number of `(JwkId, JWK)` pairs accepted from a single fetch, applied as a hard cap
/// regardless of source, to prevent an OAuth provider or the GCP endpoint from inadvertently or
/// maliciously flooding consensus with keys.
pub(crate) const MAX_JWK_KEYS_PER_FETCH: usize = 100;

/// Where the [`MAX_JWK_KEYS_PER_FETCH`] cap is enforced, relative to per-key validation,
/// active-current-epoch filtering, and per-task de-duplication ("seen").
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum JwkCapTiming {
    /// Truncate the raw fetch/parse result *before* validation/filtering/dedup. Used by GCP:
    /// capping only after dedup would let a malicious or misconfigured endpoint send far more
    /// than the cap's worth of distinctly-shaped keys and have all of them influence which
    /// keys survive dedup, defeating the point of the boundary.
    BeforeDedup,
    /// Truncate only the keys that survive validation/filtering/dedup. Used by zkLogin,
    /// unchanged from its pre-existing behavior.
    AfterDedup,
}

/// Logging/metric labels for one JWK updater loop instance.
pub(crate) struct JwkUpdaterLabels {
    /// Metric label value (the `provider` dimension) shared by `jwk_requests`,
    /// `jwk_request_errors`, `total_jwks`, `invalid_jwks`, and `unique_jwks`. This is exposed
    /// externally as a Prometheus label, so it must match each source's pre-existing value
    /// (e.g. the zkLogin `OIDCProvider`'s `Display`, or `"gcp"`).
    pub metric_label: String,
    /// Human-readable source name used only in log messages (e.g. `"JWK for provider Twitch"`,
    /// `"GCP JWKs"`). Never used for metrics.
    pub source_name: String,
}

/// Runs the shared JWK fetch/retry/validate/dedup/cap/consensus-submission loop.
///
/// This owns all *common* behavior across zkLogin and GCP JWK polling:
/// - fetches immediately on entry, then waits `fetch_interval` between later iterations;
/// - increments `jwk_requests` before every fetch attempt;
/// - on fetch failure: increments `jwk_request_errors`, logs a warning, and retries after a
///   fixed 30-second sleep (distinct from `fetch_interval`) rather than waiting a full
///   interval;
/// - on success, delegates accounting/filtering/capping to [`process_fetched_jwks`];
/// - submits each surviving key individually via `ConsensusTransaction::new_jwk_fetched`.
///
/// `fetch` performs the source-specific network fetch and parsing, returning accepted
/// `(JwkId, JWK)` pairs plus a count of entries the parser itself rejected (0 if the source
/// does not reject during parsing). `validate` is a source-specific per-key predicate (e.g.
/// provider-matching for zkLogin, size bounds for GCP); keys it rejects count toward
/// `invalid_jwks` the same as parser-rejected ones.
///
/// This function never returns on its own. Callers are expected to run it inside
/// `epoch_store.within_alive_epoch(..)` so it is cancelled at epoch boundaries, and to wrap it
/// with `tracing::Instrument` for task-specific span naming -- neither of which differs enough
/// between sources to be worth abstracting over here.
#[allow(clippy::too_many_arguments)]
pub(crate) async fn run_jwk_updater_loop<F, Fut>(
    labels: JwkUpdaterLabels,
    fetch_interval: Duration,
    cap_timing: JwkCapTiming,
    metrics: Arc<SuiNodeMetrics>,
    authority: AuthorityName,
    epoch_store: Arc<AuthorityPerEpochStore>,
    consensus_adapter: Arc<ConsensusAdapter>,
    fetch: F,
    validate: impl Fn(&JwkId, &JWK) -> bool,
) where
    F: Fn() -> Fut,
    Fut: Future<Output = SuiResult<(Vec<(JwkId, JWK)>, usize)>>,
{
    let JwkUpdaterLabels {
        metric_label,
        source_name,
    } = labels;

    // Restart-safe de-duplication happens after consensus; this is just best-effort to reduce
    // unneeded submissions within the lifetime of this task.
    let mut seen = HashSet::new();

    loop {
        jwk_log!("fetching {}", source_name);
        metrics
            .jwk_requests
            .with_label_values(&[metric_label.as_str()])
            .inc();

        match fetch().await {
            Err(e) => {
                metrics
                    .jwk_request_errors
                    .with_label_values(&[metric_label.as_str()])
                    .inc();
                warn!("Error when fetching {}: {:?}", source_name, e);
                // Retry in 30 seconds.
                tokio::time::sleep(Duration::from_secs(30)).await;
                continue;
            }
            Ok((keys, rejected_by_fetch)) => {
                let keys = process_fetched_jwks(
                    keys,
                    rejected_by_fetch,
                    cap_timing,
                    &metrics,
                    &metric_label,
                    &source_name,
                    &validate,
                    &mut seen,
                    |id, jwk| epoch_store.jwk_active_in_current_epoch(id, jwk),
                );

                for (id, jwk) in keys.into_iter() {
                    jwk_log!("Submitting {} to consensus: {:?}", source_name, id);
                    let txn = ConsensusTransaction::new_jwk_fetched(authority, id, jwk);
                    consensus_adapter
                        .submit(txn, None, &epoch_store, None, None)
                        .tap_err(|e| {
                            warn!(
                                "Error when submitting {} to consensus: {:?}",
                                source_name, e
                            )
                        })
                        .ok();
                }
            }
        }
        tokio::time::sleep(fetch_interval).await;
    }
}

/// Accounts for, validates, filters, de-duplicates, and caps one fetch's worth of `(JwkId,
/// JWK)` pairs, returning the keys that should be submitted to consensus. This is the pure
/// "common loop" filtering helper: it has no I/O of its own, so it is unit-tested directly
/// rather than only indirectly through [`run_jwk_updater_loop`].
///
/// Order of operations (metric side effects noted inline):
/// 1. `total_jwks += keys.len()` (the count *returned by* fetch/parse; a parser that already
///    rejected some entries before returning does not inflate this).
/// 2. `invalid_jwks += rejected_by_fetch`, if any (accounts for keys the parser itself
///    rejected, e.g. structurally invalid GCP JWKs).
/// 3. If `cap_timing` is [`JwkCapTiming::BeforeDedup`], truncate to
///    [`MAX_JWK_KEYS_PER_FETCH`] now.
/// 4. Retain only keys that pass `validate`, are not already active in the current epoch, and
///    have not been seen before by this task (`seen` persists across calls for the task's
///    lifetime). Each `validate` rejection adds to `invalid_jwks`; already-active and
///    already-seen keys are silently dropped without affecting `invalid_jwks`.
/// 5. `unique_jwks += keys.len()` (post-filter, pre-[`JwkCapTiming::AfterDedup`]-cap count).
/// 6. If `cap_timing` is [`JwkCapTiming::AfterDedup`], truncate to [`MAX_JWK_KEYS_PER_FETCH`]
///    now.
#[allow(clippy::too_many_arguments)]
fn process_fetched_jwks(
    mut keys: Vec<(JwkId, JWK)>,
    rejected_by_fetch: usize,
    cap_timing: JwkCapTiming,
    metrics: &SuiNodeMetrics,
    metric_label: &str,
    source_name: &str,
    validate: &impl Fn(&JwkId, &JWK) -> bool,
    seen: &mut HashSet<(JwkId, JWK)>,
    is_active_in_current_epoch: impl Fn(&JwkId, &JWK) -> bool,
) -> Vec<(JwkId, JWK)> {
    metrics
        .total_jwks
        .with_label_values(&[metric_label])
        .inc_by(keys.len() as u64);
    if rejected_by_fetch > 0 {
        metrics
            .invalid_jwks
            .with_label_values(&[metric_label])
            .inc_by(rejected_by_fetch as u64);
    }

    if cap_timing == JwkCapTiming::BeforeDedup {
        enforce_jwk_key_limit(&mut keys, MAX_JWK_KEYS_PER_FETCH, source_name);
    }

    let mut invalid_count = 0u64;
    keys.retain(|(id, jwk)| {
        if !validate(id, jwk) {
            invalid_count += 1;
            return false;
        }
        !is_active_in_current_epoch(id, jwk) && seen.insert((id.clone(), jwk.clone()))
    });
    if invalid_count > 0 {
        metrics
            .invalid_jwks
            .with_label_values(&[metric_label])
            .inc_by(invalid_count);
    }

    metrics
        .unique_jwks
        .with_label_values(&[metric_label])
        .inc_by(keys.len() as u64);

    if cap_timing == JwkCapTiming::AfterDedup {
        enforce_jwk_key_limit(&mut keys, MAX_JWK_KEYS_PER_FETCH, source_name);
    }

    keys
}

/// Truncates `keys` to at most `max` entries, logging a warning if it does. See
/// [`JwkCapTiming`] for why *where* this is called from matters.
fn enforce_jwk_key_limit(keys: &mut Vec<(JwkId, JWK)>, max: usize, source_name: &str) {
    if keys.len() > max {
        warn!(
            "{} sent too many JWKs, only the first {} will be used",
            source_name, max
        );
        keys.truncate(max);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use prometheus::Registry;

    fn test_jwk(iss: &str, kid: &str) -> (JwkId, JWK) {
        (
            JwkId {
                iss: iss.to_string(),
                kid: kid.to_string(),
            },
            JWK {
                kty: "RSA".to_string(),
                e: "AQAB".to_string(),
                n: "n".to_string(),
                alg: "RS256".to_string(),
            },
        )
    }

    fn test_metrics() -> SuiNodeMetrics {
        SuiNodeMetrics::new(&Registry::new())
    }

    const LABEL: &str = "test-source";

    fn always_valid(_id: &JwkId, _jwk: &JWK) -> bool {
        true
    }

    fn never_active(_id: &JwkId, _jwk: &JWK) -> bool {
        false
    }

    #[test]
    fn accepts_and_counts_fresh_keys() {
        let m = test_metrics();
        let mut seen = HashSet::new();
        let keys = vec![test_jwk("iss1", "a"), test_jwk("iss1", "b")];

        let out = process_fetched_jwks(
            keys,
            0,
            JwkCapTiming::AfterDedup,
            &m,
            LABEL,
            LABEL,
            &always_valid,
            &mut seen,
            never_active,
        );

        assert_eq!(out.len(), 2);
        assert_eq!(m.total_jwks.with_label_values(&[LABEL]).get(), 2);
        assert_eq!(m.unique_jwks.with_label_values(&[LABEL]).get(), 2);
        assert_eq!(m.invalid_jwks.with_label_values(&[LABEL]).get(), 0);
    }

    /// A parser (e.g. `parse_gcp_jwks`) that rejects some entries before returning must have
    /// its rejected count reflected in `invalid_jwks` by the common loop, while `total_jwks`
    /// only counts what was actually returned -- matching the pre-existing GCP semantics where
    /// `total_jwks` never included parser-rejected keys.
    #[test]
    fn parser_rejected_count_increments_invalid_jwks_but_not_total_jwks() {
        let m = test_metrics();
        let mut seen = HashSet::new();
        let keys = vec![test_jwk("iss1", "a")];

        let out = process_fetched_jwks(
            keys,
            3,
            JwkCapTiming::AfterDedup,
            &m,
            LABEL,
            LABEL,
            &always_valid,
            &mut seen,
            never_active,
        );

        assert_eq!(out.len(), 1);
        assert_eq!(m.total_jwks.with_label_values(&[LABEL]).get(), 1);
        assert_eq!(m.invalid_jwks.with_label_values(&[LABEL]).get(), 3);
    }

    #[test]
    fn source_invalid_keys_are_dropped_and_counted() {
        let m = test_metrics();
        let mut seen = HashSet::new();
        let keys = vec![test_jwk("iss1", "a"), test_jwk("iss1", "b")];
        let validate = |id: &JwkId, _jwk: &JWK| id.kid != "b";

        let out = process_fetched_jwks(
            keys,
            0,
            JwkCapTiming::AfterDedup,
            &m,
            LABEL,
            LABEL,
            &validate,
            &mut seen,
            never_active,
        );

        assert_eq!(out.len(), 1);
        assert_eq!(out[0].0.kid, "a");
        assert_eq!(m.invalid_jwks.with_label_values(&[LABEL]).get(), 1);
    }

    /// Oversized (per-key) rejection is just another `validate` failure from the caller's
    /// perspective, and must be counted the same way as any other source-specific validation
    /// failure.
    #[test]
    fn oversized_keys_are_dropped_and_counted_like_any_other_validation_failure() {
        let m = test_metrics();
        let mut seen = HashSet::new();
        let keys = vec![test_jwk("iss1", "a"), test_jwk("iss1", "oversized")];
        let validate = |id: &JwkId, _jwk: &JWK| id.kid != "oversized";

        let out = process_fetched_jwks(
            keys,
            0,
            JwkCapTiming::BeforeDedup,
            &m,
            LABEL,
            LABEL,
            &validate,
            &mut seen,
            never_active,
        );

        assert_eq!(out.len(), 1);
        assert_eq!(m.invalid_jwks.with_label_values(&[LABEL]).get(), 1);
    }

    #[test]
    fn already_active_keys_are_dropped_without_invalid_metric() {
        let m = test_metrics();
        let mut seen = HashSet::new();
        let keys = vec![test_jwk("iss1", "a")];
        let is_active = |_id: &JwkId, _jwk: &JWK| true;

        let out = process_fetched_jwks(
            keys,
            0,
            JwkCapTiming::AfterDedup,
            &m,
            LABEL,
            LABEL,
            &always_valid,
            &mut seen,
            is_active,
        );

        assert!(out.is_empty());
        assert_eq!(m.invalid_jwks.with_label_values(&[LABEL]).get(), 0);
        assert_eq!(m.unique_jwks.with_label_values(&[LABEL]).get(), 0);
    }

    #[test]
    fn duplicate_within_one_batch_is_deduped_by_seen() {
        let m = test_metrics();
        let mut seen = HashSet::new();
        let keys = vec![test_jwk("iss1", "a"), test_jwk("iss1", "a")];

        let out = process_fetched_jwks(
            keys,
            0,
            JwkCapTiming::AfterDedup,
            &m,
            LABEL,
            LABEL,
            &always_valid,
            &mut seen,
            never_active,
        );

        assert_eq!(out.len(), 1);
        assert_eq!(m.unique_jwks.with_label_values(&[LABEL]).get(), 1);
    }

    /// `seen` is passed in by the caller and must persist across multiple fetch cycles for the
    /// lifetime of the task (this is what the "restart-safe de-duplication happens after
    /// consensus, this is just best-effort" comment in the loop relies on).
    #[test]
    fn duplicate_across_batches_is_deduped_by_persistent_seen() {
        let m = test_metrics();
        let mut seen = HashSet::new();

        let first = process_fetched_jwks(
            vec![test_jwk("iss1", "a")],
            0,
            JwkCapTiming::AfterDedup,
            &m,
            LABEL,
            LABEL,
            &always_valid,
            &mut seen,
            never_active,
        );
        assert_eq!(first.len(), 1);

        let second = process_fetched_jwks(
            vec![test_jwk("iss1", "a")],
            0,
            JwkCapTiming::AfterDedup,
            &m,
            LABEL,
            LABEL,
            &always_valid,
            &mut seen,
            never_active,
        );
        assert!(
            second.is_empty(),
            "seen must persist across poll cycles for the lifetime of the task"
        );
    }

    #[test]
    fn cap_after_dedup_keeps_only_max_survivors_but_unique_jwks_counts_pre_cap() {
        let m = test_metrics();
        let mut seen = HashSet::new();
        let keys: Vec<_> = (0..150)
            .map(|i| test_jwk("iss1", &format!("k{i}")))
            .collect();

        let out = process_fetched_jwks(
            keys,
            0,
            JwkCapTiming::AfterDedup,
            &m,
            LABEL,
            LABEL,
            &always_valid,
            &mut seen,
            never_active,
        );

        assert_eq!(out.len(), MAX_JWK_KEYS_PER_FETCH);
        assert_eq!(m.unique_jwks.with_label_values(&[LABEL]).get(), 150);
    }

    /// This is the key ordering regression test: with `BeforeDedup`, the cap runs on the raw
    /// fetch result first. 100 identical duplicates plus 50 distinct extra keys must collapse
    /// to a single survivor (the 50 "extra" keys are truncated away before dedup ever runs and
    /// so are never considered), rather than the up-to-100 distinct survivors that an
    /// after-dedup cap would have produced from the same input.
    #[test]
    fn cap_before_dedup_truncates_the_raw_fetch_result_before_seen_can_thin_it_out() {
        let m = test_metrics();
        let mut seen = HashSet::new();
        let mut keys: Vec<_> = std::iter::repeat_with(|| test_jwk("iss1", "dup"))
            .take(MAX_JWK_KEYS_PER_FETCH)
            .collect();
        keys.extend((0..50).map(|i| test_jwk("iss1", &format!("extra{i}"))));
        assert_eq!(keys.len(), MAX_JWK_KEYS_PER_FETCH + 50);

        let out = process_fetched_jwks(
            keys,
            0,
            JwkCapTiming::BeforeDedup,
            &m,
            LABEL,
            LABEL,
            &always_valid,
            &mut seen,
            never_active,
        );

        assert_eq!(
            out.len(),
            1,
            "the 50 distinct 'extra' keys must never be considered: the cap truncated them \
             away before dedup ran"
        );
    }

    #[test]
    fn enforce_jwk_key_limit_is_noop_under_limit() {
        let mut keys: Vec<_> = (0..MAX_JWK_KEYS_PER_FETCH)
            .map(|i| test_jwk("iss1", &format!("k{i}")))
            .collect();
        enforce_jwk_key_limit(&mut keys, MAX_JWK_KEYS_PER_FETCH, "test");
        assert_eq!(keys.len(), MAX_JWK_KEYS_PER_FETCH);
    }

    #[test]
    fn enforce_jwk_key_limit_truncates_over_limit() {
        let mut keys: Vec<_> = (0..MAX_JWK_KEYS_PER_FETCH + 1)
            .map(|i| test_jwk("iss1", &format!("k{i}")))
            .collect();
        enforce_jwk_key_limit(&mut keys, MAX_JWK_KEYS_PER_FETCH, "test");
        assert_eq!(keys.len(), MAX_JWK_KEYS_PER_FETCH);
    }
}
