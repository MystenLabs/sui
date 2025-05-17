// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use std::sync::Arc;

use anyhow::Context as _;
use async_graphql::{dataloader::DataLoader, Context, Object};
use sui_indexer_alt_reader::{
    epochs::{EpochEndKey, EpochStartKey},
    pg_reader::PgReader,
};
use sui_indexer_alt_schema::epochs::{StoredEpochEnd, StoredEpochStart};

use crate::{
    api::scalars::{big_int::BigInt, date_time::DateTime, uint53::UInt53},
    error::RpcError,
    scope::Scope,
};

use super::protocol_configs::ProtocolConfigs;

pub(crate) struct Epoch {
    pub(crate) epoch_id: u64,
    pub(crate) scope: Scope,
    start: EpochStart,
}

#[derive(Clone)]
struct EpochStart {
    contents: Option<Arc<StoredEpochStart>>,
}

#[derive(Clone)]
struct EpochEnd {
    contents: Option<Arc<StoredEpochEnd>>,
}

/// Activity on Sui is partitioned in time, into epochs.
///
/// Epoch changes are opportunities for the network to reconfigure itself (perform protocol or system package upgrades, or change the committee) and distribute staking rewards. The network aims to keep epochs roughly the same duration as each other.
///
/// During a particular epoch the following data is fixed:
///
/// - protocol version,
/// - reference gas price,
/// - system package versions,
/// - validators in the committee.
#[Object]
impl Epoch {
    /// The epoch's id as a sequence number that starts at 0 and is incremented by one at every epoch change.
    async fn epoch_id(&self) -> UInt53 {
        self.epoch_id.into()
    }

    #[graphql(flatten)]
    async fn start(&self, ctx: &Context<'_>) -> Result<EpochStart, RpcError> {
        self.start.fetch(ctx, &self.scope, self.epoch_id).await
    }

    #[graphql(flatten)]
    async fn end(&self, ctx: &Context<'_>) -> Result<EpochEnd, RpcError> {
        EpochEnd::fetch(ctx, &self.scope, self.epoch_id).await
    }
}

#[Object]
impl EpochStart {
    /// The epoch's corresponding protocol configuration, including the feature flags and the configuration options.
    async fn protocol_configs(&self) -> Option<ProtocolConfigs> {
        let Some(contents) = &self.contents else {
            return None;
        };

        Some(ProtocolConfigs::with_protocol_version(
            contents.protocol_version as u64,
        ))
    }

    /// The minimum gas price that a quorum of validators are guaranteed to sign a transaction for in this epoch.
    async fn reference_gas_price(&self) -> Option<BigInt> {
        let Some(contents) = &self.contents else {
            return None;
        };

        Some(BigInt::from(contents.reference_gas_price))
    }

    /// The timestamp associated with the first checkpoint in the epoch.
    async fn start_timestamp(&self) -> Result<Option<DateTime>, RpcError> {
        let Some(contents) = &self.contents else {
            return Ok(None);
        };

        Ok(Some(DateTime::from_ms(contents.start_timestamp_ms)?))
    }
}

#[Object]
impl EpochEnd {
    /// The timestamp associated with the last checkpoint in the epoch (or `null` if the epoch has not finished yet).
    async fn end_timestamp(&self) -> Result<Option<DateTime>, RpcError> {
        let Some(contents) = &self.contents else {
            return Ok(None);
        };

        Ok(Some(DateTime::from_ms(contents.end_timestamp_ms)?))
    }
}

impl Epoch {
    /// Construct an epoch that is represented by just its identifier (its sequence number). This
    /// does not check whether the epoch exists, so should not be used to "fetch" an epoch based on
    /// an ID provided as user input.
    pub(crate) fn with_id(scope: Scope, epoch_id: u64) -> Self {
        Self {
            epoch_id,
            scope,
            start: EpochStart::empty(),
        }
    }

    /// Load the epoch from the store, and return it fully inflated (with contents already
    /// fetched). Returns `None` if the epoch does not exist (or started after the checkpoint being
    /// viewed).
    pub(crate) async fn fetch(
        ctx: &Context<'_>,
        scope: Scope,
        epoch_id: UInt53,
    ) -> Result<Option<Self>, RpcError> {
        let start = EpochStart::empty()
            .fetch(ctx, &scope, epoch_id.into())
            .await?;

        let Some(contents) = &start.contents else {
            return Ok(None);
        };

        Ok(Some(Self {
            epoch_id: contents.epoch as u64,
            scope,
            start,
        }))
    }
}

impl EpochStart {
    fn empty() -> Self {
        Self { contents: None }
    }

    /// Attempt to fill the contents. If the contents are already filled, returns a clone,
    /// otherwise attempts to fetch from the store. The resulting value may still have an empty
    /// contents field, because it could not be found in the store, or the epoch started after the
    /// checkpoint being viewed.
    async fn fetch(
        &self,
        ctx: &Context<'_>,
        scope: &Scope,
        epoch_id: u64,
    ) -> Result<Self, RpcError> {
        if self.contents.is_some() {
            return Ok(self.clone());
        }

        let pg_loader: &Arc<DataLoader<PgReader>> = ctx.data()?;
        let Some(stored) = pg_loader
            .load_one(EpochStartKey(epoch_id))
            .await
            .context("Failed to fetch epoch start information")?
        else {
            return Ok(self.clone());
        };

        if stored.cp_lo as u64 > scope.checkpoint_viewed_at() {
            return Ok(self.clone());
        }

        Ok(Self {
            contents: Some(Arc::new(stored)),
        })
    }
}

impl EpochEnd {
    fn empty() -> Self {
        Self { contents: None }
    }

    /// Attempt to fetch information about the end of an epoch from the store. May return an empty
    /// response if the epoch has not ended yet, as of the checkpoint being viewed.
    async fn fetch(ctx: &Context<'_>, scope: &Scope, epoch_id: u64) -> Result<Self, RpcError> {
        let pg_loader: &Arc<DataLoader<PgReader>> = ctx.data()?;
        let Some(stored) = pg_loader
            .load_one(EpochEndKey(epoch_id))
            .await
            .context("Failed to fetch epoch end information")?
        else {
            return Ok(Self::empty());
        };

        if stored.cp_hi as u64 > scope.checkpoint_viewed_at() {
            return Ok(Self::empty());
        }

        Ok(Self {
            contents: Some(Arc::new(stored)),
        })
    }
}
