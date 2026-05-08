// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

//! Shared test suites for implementations of [`Connection`], [`ConcurrentConnection`], and
//! [`SequentialConnection`].
//!
//! A caller implements [`Harness`] in their `#[cfg(test)] mod tests`, then invokes one or more of
//! [`connection_tests!`], [`concurrent_connection_tests!`], and [`sequential_connection_tests!`]
//! to generate trait-level tests collected across existing impls.
//!
//! The generated tests are thin `#[tokio::test]` wrappers that create a fresh harness and delegate
//! to default trait methods in this module. Keeping the assertions in trait methods means `assert!`
//! failures report the shared test source line here, not the caller's macro invocation line.

use std::time::Duration;

use async_trait::async_trait;
use scoped_futures::ScopedFutureExt;

use crate::CommitterWatermark;
use crate::ConcurrentConnection;
use crate::ConcurrentStore;
use crate::Connection;
use crate::ReaderWatermark;
use crate::SequentialStore;
use crate::Store;

pub mod mock_store;

const PIPELINE: &str = "pipeline";
const EPOCH_HI: u64 = 7;
const CHECKPOINT_HI: u64 = 200;
const TX_HI: u64 = 42;
const TIMESTAMP_MS_HI: u64 = 99;
const READER_LO: u64 = 123;
const PRUNER_HI: u64 = 77;

// =============================================================================
// Test harness traits
// =============================================================================

#[async_trait(?Send)]
pub trait Harness {
    type Store: Store;

    async fn new() -> Self;

    fn store(&self) -> &Self::Store;

    async fn connect<'c>(&'c self) -> <Self::Store as Store>::Connection<'c> {
        self.store().connect().await.unwrap()
    }

    /// Verifies fresh initialization without a checkpoint returns no committed checkpoint.
    async fn init_watermark_fresh_without_checkpoint(&self) {
        let mut conn = self.connect().await;
        let init = conn.init_watermark(PIPELINE, None).await.unwrap().unwrap();
        assert_eq!(init.checkpoint_hi_inclusive, None);
    }

    /// Verifies fresh initialization with a checkpoint returns that checkpoint.
    async fn init_watermark_fresh_with_checkpoint(&self) {
        let mut conn = self.connect().await;
        let init = conn
            .init_watermark(PIPELINE, Some(CHECKPOINT_HI))
            .await
            .unwrap()
            .unwrap();
        assert_eq!(init.checkpoint_hi_inclusive, Some(CHECKPOINT_HI));
    }

    /// Verifies initialization returns an existing checkpoint instead of overwriting it.
    async fn init_watermark_returns_existing_on_conflict(&self) {
        let mut conn = self.connect().await;
        let fixture = committer_watermark_fixture();

        conn.init_watermark(PIPELINE, Some(fixture.checkpoint_hi_inclusive))
            .await
            .unwrap();
        conn.set_committer_watermark(PIPELINE, fixture)
            .await
            .unwrap();

        let second = conn
            .init_watermark(PIPELINE, Some(0))
            .await
            .unwrap()
            .unwrap();
        assert_eq!(
            second.checkpoint_hi_inclusive,
            Some(fixture.checkpoint_hi_inclusive)
        );
    }

    /// Verifies an initialized pipeline has no committer watermark until one is written.
    async fn committer_watermark_initial_is_none(&self) {
        let mut conn = self.connect().await;

        conn.init_watermark(PIPELINE, None).await.unwrap();
        assert!(conn.committer_watermark(PIPELINE).await.unwrap().is_none());
    }

    /// Verifies a committer watermark write persists every committer field.
    async fn committer_watermark_roundtrip(&self) {
        let mut conn = self.connect().await;
        let fixture = committer_watermark_fixture();

        conn.init_watermark(PIPELINE, None).await.unwrap();
        assert!(
            conn.set_committer_watermark(PIPELINE, fixture)
                .await
                .unwrap()
        );

        let stored = conn.committer_watermark(PIPELINE).await.unwrap().unwrap();
        assert_eq!(stored, fixture);
    }

    /// Verifies a committer watermark update can advance to a higher checkpoint.
    async fn set_committer_watermark_advances(&self) {
        let mut conn = self.connect().await;
        let fixture = committer_watermark_fixture();

        conn.init_watermark(PIPELINE, None).await.unwrap();

        let lower = CommitterWatermark {
            checkpoint_hi_inclusive: fixture.checkpoint_hi_inclusive / 2,
            ..fixture
        };
        assert!(conn.set_committer_watermark(PIPELINE, lower).await.unwrap());
        assert!(
            conn.set_committer_watermark(PIPELINE, fixture)
                .await
                .unwrap()
        );

        let stored = conn.committer_watermark(PIPELINE).await.unwrap().unwrap();
        assert_eq!(stored, fixture);
    }

    /// Verifies `checkpoint_hi_inclusive` should not regress and stale writes leave stored state
    /// unchanged.
    async fn set_committer_watermark_rejects_regression(&self) {
        let mut conn = self.connect().await;
        let fixture = committer_watermark_fixture();

        conn.init_watermark(PIPELINE, None).await.unwrap();
        assert!(
            conn.set_committer_watermark(PIPELINE, fixture)
                .await
                .unwrap()
        );

        let regressed = CommitterWatermark {
            epoch_hi_inclusive: fixture.epoch_hi_inclusive + 1,
            checkpoint_hi_inclusive: fixture.checkpoint_hi_inclusive / 2,
            tx_hi: fixture.tx_hi + 1,
            timestamp_ms_hi_inclusive: fixture.timestamp_ms_hi_inclusive + 1,
        };
        assert!(
            !conn
                .set_committer_watermark(PIPELINE, regressed)
                .await
                .unwrap()
        );

        let stored = conn.committer_watermark(PIPELINE).await.unwrap().unwrap();
        assert_eq!(stored, fixture);
    }

    /// Verifies the first chain id check stores the id and accepts it.
    async fn accepts_chain_id_first_call_writes_and_accepts(&self) {
        let mut conn = self.connect().await;
        conn.init_watermark(PIPELINE, None).await.unwrap();
        assert!(conn.accepts_chain_id(PIPELINE, [1u8; 32]).await.unwrap());
    }

    /// Verifies a matching chain id remains accepted after it has been stored.
    async fn accepts_chain_id_matching_accepts(&self) {
        let mut conn = self.connect().await;
        conn.init_watermark(PIPELINE, None).await.unwrap();
        let chain_id = [1u8; 32];
        assert!(conn.accepts_chain_id(PIPELINE, chain_id).await.unwrap());
        assert!(conn.accepts_chain_id(PIPELINE, chain_id).await.unwrap());
    }

    /// Verifies a mismatched chain id is rejected without replacing the stored id.
    async fn accepts_chain_id_mismatching_rejects(&self) {
        let mut conn = self.connect().await;
        conn.init_watermark(PIPELINE, None).await.unwrap();
        let chain_id_a = [1u8; 32];
        let chain_id_b = [2u8; 32];
        assert!(conn.accepts_chain_id(PIPELINE, chain_id_a).await.unwrap());
        assert!(!conn.accepts_chain_id(PIPELINE, chain_id_b).await.unwrap());
        assert!(conn.accepts_chain_id(PIPELINE, chain_id_a).await.unwrap());
    }

    /// Verifies chain ids are scoped independently by pipeline.
    async fn accepts_chain_id_distinct_pipelines(&self) {
        let mut conn = self.connect().await;
        conn.init_watermark("a", None).await.unwrap();
        conn.init_watermark("b", None).await.unwrap();
        let chain_id_a = [1u8; 32];
        let chain_id_b = [2u8; 32];
        assert!(conn.accepts_chain_id("a", chain_id_a).await.unwrap());
        assert!(conn.accepts_chain_id("b", chain_id_b).await.unwrap());
        assert!(!conn.accepts_chain_id("a", chain_id_b).await.unwrap());
        assert!(conn.accepts_chain_id("a", chain_id_a).await.unwrap());
        assert!(conn.accepts_chain_id("b", chain_id_b).await.unwrap());
    }
}

#[async_trait(?Send)]
pub trait ConcurrentHarness: Harness
where
    Self::Store: ConcurrentStore,
{
    /// Verifies a reader watermark can be read and then advanced.
    async fn reader_watermark_roundtrip(&self) {
        let mut conn = self.connect().await;
        concurrent_bootstrap(&mut conn, CHECKPOINT_HI).await;

        let watermark = conn.reader_watermark(PIPELINE).await.unwrap().unwrap();
        assert_eq!(
            watermark,
            ReaderWatermark {
                reader_lo: 0,
                ..reader_watermark_fixture()
            }
        );

        assert!(
            conn.set_reader_watermark(PIPELINE, READER_LO)
                .await
                .unwrap()
        );
        let watermark = conn.reader_watermark(PIPELINE).await.unwrap().unwrap();
        assert_eq!(watermark, reader_watermark_fixture());
    }

    /// Verifies a pruner watermark update is visible to subsequent reads.
    async fn set_pruner_watermark_roundtrip(&self) {
        let mut conn = self.connect().await;
        concurrent_bootstrap(&mut conn, CHECKPOINT_HI).await;

        assert!(
            conn.set_pruner_watermark(PIPELINE, PRUNER_HI)
                .await
                .unwrap()
        );
        let watermark = conn
            .pruner_watermark(PIPELINE, Duration::ZERO)
            .await
            .unwrap()
            .unwrap();
        assert_eq!(watermark.pruner_hi, PRUNER_HI);
    }

    /// Verifies `reader_lo` should not regress: equal and lower values are rejected.
    async fn set_reader_watermark_rejects_stale(&self) {
        let mut conn = self.connect().await;
        concurrent_bootstrap(&mut conn, CHECKPOINT_HI).await;

        assert!(
            conn.set_reader_watermark(PIPELINE, READER_LO)
                .await
                .unwrap()
        );
        assert!(
            !conn
                .set_reader_watermark(PIPELINE, READER_LO)
                .await
                .unwrap(),
            "equal reader_lo must be rejected"
        );
        assert!(
            !conn
                .set_reader_watermark(PIPELINE, READER_LO - 1)
                .await
                .unwrap(),
            "lower reader_lo must be rejected"
        );
        let watermark = conn.reader_watermark(PIPELINE).await.unwrap().unwrap();
        assert_eq!(watermark, reader_watermark_fixture());
    }

    /// Verifies `pruner_hi` should not regress: equal and lower values are rejected.
    async fn set_pruner_watermark_rejects_stale(&self) {
        let mut conn = self.connect().await;
        concurrent_bootstrap(&mut conn, CHECKPOINT_HI).await;

        assert!(
            conn.set_pruner_watermark(PIPELINE, PRUNER_HI)
                .await
                .unwrap()
        );
        assert!(
            !conn
                .set_pruner_watermark(PIPELINE, PRUNER_HI)
                .await
                .unwrap(),
            "equal pruner_hi must be rejected"
        );
        assert!(
            !conn
                .set_pruner_watermark(PIPELINE, PRUNER_HI - 1)
                .await
                .unwrap(),
            "lower pruner_hi must be rejected"
        );
        let watermark = conn
            .pruner_watermark(PIPELINE, Duration::ZERO)
            .await
            .unwrap()
            .unwrap();
        assert_eq!(watermark.pruner_hi, PRUNER_HI);
    }
}

impl<T> ConcurrentHarness for T
where
    T: Harness,
    T::Store: ConcurrentStore,
{
}

#[async_trait(?Send)]
pub trait SequentialHarness: Harness
where
    Self::Store: SequentialStore,
{
    /// Verifies a failed sequential transaction rolls back writes made inside the transaction.
    async fn transaction_rolls_back_on_error(&self) {
        let fixture = committer_watermark_fixture();

        {
            let mut conn = self.connect().await;
            conn.init_watermark(PIPELINE, None).await.unwrap();
            assert!(
                conn.set_committer_watermark(PIPELINE, fixture)
                    .await
                    .unwrap()
            );
        }

        let result: anyhow::Result<()> = self
            .store()
            .transaction(|conn| {
                async move {
                    let advanced = CommitterWatermark {
                        checkpoint_hi_inclusive: fixture.checkpoint_hi_inclusive + 1,
                        ..fixture
                    };
                    let _ = conn.set_committer_watermark(PIPELINE, advanced).await?;
                    Err(anyhow::anyhow!("rollback"))
                }
                .scope_boxed()
            })
            .await;
        assert!(result.is_err());

        let mut conn = self.connect().await;
        let stored = conn.committer_watermark(PIPELINE).await.unwrap().unwrap();
        assert_eq!(stored, fixture);
    }
}

impl<T> SequentialHarness for T
where
    T: Harness,
    T::Store: SequentialStore,
{
}

// =============================================================================
// Internal test helpers
// =============================================================================

fn committer_watermark_fixture() -> CommitterWatermark {
    CommitterWatermark {
        epoch_hi_inclusive: EPOCH_HI,
        checkpoint_hi_inclusive: CHECKPOINT_HI,
        tx_hi: TX_HI,
        timestamp_ms_hi_inclusive: TIMESTAMP_MS_HI,
    }
}

fn reader_watermark_fixture() -> ReaderWatermark {
    ReaderWatermark {
        checkpoint_hi_inclusive: CHECKPOINT_HI,
        reader_lo: READER_LO,
    }
}

async fn concurrent_bootstrap<C: ConcurrentConnection>(conn: &mut C, checkpoint_hi_inclusive: u64) {
    conn.init_watermark(PIPELINE, None).await.unwrap();
    conn.set_committer_watermark(
        PIPELINE,
        CommitterWatermark {
            epoch_hi_inclusive: 0,
            checkpoint_hi_inclusive,
            tx_hi: 0,
            timestamp_ms_hi_inclusive: 0,
        },
    )
    .await
    .unwrap();
}

// =============================================================================
// Trait test macros — generate `#[tokio::test]` wrappers that delegate to harness trait methods.
// =============================================================================

#[doc(hidden)]
#[macro_export]
macro_rules! __test_case {
    ($Harness:ty, $Trait:ident::$case:ident) => {
        #[::tokio::test]
        async fn $case() {
            let harness = <$Harness as $crate::testing::Harness>::new().await;
            <$Harness as $crate::testing::$Trait>::$case(&harness).await;
        }
    };
}

/// Generate [`Connection`] trait tests against a caller-provided [`Harness`] type.
///
/// The harness is recreated for each generated test.
///
/// # Example
///
/// ```ignore
/// struct MyHarness { store: MyStore }
///
/// sui_indexer_alt_framework_store_traits::connection_tests!(MyHarness);
/// ```
#[macro_export]
macro_rules! connection_tests {
    ($Harness:ty $(,)?) => {
        mod connection_tests {
            use super::*;

            $crate::__test_case!($Harness, Harness::init_watermark_fresh_without_checkpoint);
            $crate::__test_case!($Harness, Harness::init_watermark_fresh_with_checkpoint);
            $crate::__test_case!(
                $Harness,
                Harness::init_watermark_returns_existing_on_conflict
            );
            $crate::__test_case!($Harness, Harness::committer_watermark_initial_is_none);
            $crate::__test_case!($Harness, Harness::committer_watermark_roundtrip);
            $crate::__test_case!($Harness, Harness::set_committer_watermark_advances);
            $crate::__test_case!(
                $Harness,
                Harness::set_committer_watermark_rejects_regression
            );
            $crate::__test_case!(
                $Harness,
                Harness::accepts_chain_id_first_call_writes_and_accepts
            );
            $crate::__test_case!($Harness, Harness::accepts_chain_id_matching_accepts);
            $crate::__test_case!($Harness, Harness::accepts_chain_id_mismatching_rejects);
            $crate::__test_case!($Harness, Harness::accepts_chain_id_distinct_pipelines);
        }
    };
}

/// Generate [`ConcurrentConnection`] trait tests against a caller-provided [`Harness`] type.
///
/// The harness is recreated for each generated test.
#[macro_export]
macro_rules! concurrent_connection_tests {
    ($Harness:ty $(,)?) => {
        mod concurrent_connection_tests {
            use super::*;

            $crate::__test_case!($Harness, ConcurrentHarness::reader_watermark_roundtrip);
            $crate::__test_case!($Harness, ConcurrentHarness::set_pruner_watermark_roundtrip);
            $crate::__test_case!(
                $Harness,
                ConcurrentHarness::set_reader_watermark_rejects_stale
            );
            $crate::__test_case!(
                $Harness,
                ConcurrentHarness::set_pruner_watermark_rejects_stale
            );
        }
    };
}

/// Generate [`SequentialConnection`] trait tests against a caller-provided [`Harness`] type.
///
/// The harness is recreated for each generated test.
#[macro_export]
macro_rules! sequential_connection_tests {
    ($Harness:ty $(,)?) => {
        mod sequential_connection_tests {
            use super::*;

            $crate::__test_case!($Harness, SequentialHarness::transaction_rolls_back_on_error);
        }
    };
}

#[cfg(test)]
mod tests {
    //! Exercise every macro arm against `MockStore` to prove the macros compile and the test
    //! bodies behave correctly — without requiring the framework crate.

    use super::Harness;
    use super::mock_store::MockStore;

    struct MockHarness {
        store: MockStore,
    }

    #[async_trait::async_trait(?Send)]
    impl Harness for MockHarness {
        type Store = MockStore;

        async fn new() -> Self {
            Self {
                store: MockStore::default(),
            }
        }

        fn store(&self) -> &Self::Store {
            &self.store
        }
    }

    connection_tests!(MockHarness);
    concurrent_connection_tests!(MockHarness);
    sequential_connection_tests!(MockHarness);
}
