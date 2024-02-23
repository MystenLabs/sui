// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use crate::{
    config::{DEFAULT_REQUEST_TIMEOUT_MS, DEFAULT_SERVER_DB_POOL_SIZE},
    error::Error,
    types::{address::Address, sui_address::SuiAddress, validator::Validator},
};
use std::{collections::BTreeMap, time::Duration};
use sui_indexer::db::PgConnectionPoolConfig;
use sui_indexer::{apis::GovernanceReadApi, indexer_reader::IndexerReader};
use sui_json_rpc_types::Stake as RpcStakedSui;
use sui_types::{
    base_types::SuiAddress as NativeSuiAddress,
    governance::StakedSui as NativeStakedSui,
    sui_system_state::sui_system_state_summary::{
        SuiSystemStateSummary as NativeSuiSystemStateSummary, SuiValidatorSummary,
    },
};

pub(crate) struct PgManager {
    pub inner: IndexerReader,
}

impl PgManager {
    pub(crate) fn new(inner: IndexerReader) -> Self {
        Self { inner }
    }

    /// Create a new underlying reader, which is used by this type as well as other data providers.
    pub(crate) fn reader(db_url: impl Into<String>) -> Result<IndexerReader, Error> {
        Self::reader_with_config(
            db_url,
            DEFAULT_SERVER_DB_POOL_SIZE,
            DEFAULT_REQUEST_TIMEOUT_MS,
        )
    }

    pub(crate) fn reader_with_config(
        db_url: impl Into<String>,
        pool_size: u32,
        timeout_ms: u64,
    ) -> Result<IndexerReader, Error> {
        let mut config = PgConnectionPoolConfig::default();
        config.set_pool_size(pool_size);
        config.set_statement_timeout(Duration::from_millis(timeout_ms));
        IndexerReader::new_with_config(db_url, config)
            .map_err(|e| Error::Internal(format!("Failed to create reader: {e}")))
    }
}

/// Implement methods to be used by graphql resolvers
impl PgManager {
    /// Retrieve the validator APYs
    pub(crate) async fn fetch_validator_apys(
        &self,
        address: &NativeSuiAddress,
    ) -> Result<Option<f64>, Error> {
        let governance_api = GovernanceReadApi::new(self.inner.clone());

        governance_api
            .get_validator_apy(address)
            .await
            .map_err(|e| Error::Internal(format!("{e}")))
    }

    pub(crate) async fn available_range(&self) -> Result<(u64, u64), Error> {
        Ok(self
            .inner
            .spawn_blocking(|this| this.get_consistent_read_range())
            .await
            .map(|(start, end)| (start as u64, end as u64))?)
    }

    /// If no epoch was requested or if the epoch requested is in progress,
    /// returns the latest sui system state.
    pub(crate) async fn fetch_sui_system_state(
        &self,
        epoch_id: Option<u64>,
    ) -> Result<NativeSuiSystemStateSummary, Error> {
        let latest_sui_system_state = self
            .inner
            .spawn_blocking(move |this| this.get_latest_sui_system_state())
            .await?;

        if epoch_id.is_some_and(|id| id == latest_sui_system_state.epoch) {
            Ok(latest_sui_system_state)
        } else {
            Ok(self
                .inner
                .spawn_blocking(move |this| this.get_epoch_sui_system_state(epoch_id))
                .await?)
        }
    }

    /// Make a request to the RPC for its representations of the staked sui we parsed out of the
    /// object.  Used to implement fields that are implemented in JSON-RPC but not GraphQL (yet).
    pub(crate) async fn fetch_rpc_staked_sui(
        &self,
        stake: NativeStakedSui,
    ) -> Result<RpcStakedSui, Error> {
        let governance_api = GovernanceReadApi::new(self.inner.clone());

        let mut delegated_stakes = governance_api
            .get_delegated_stakes(vec![stake])
            .await
            .map_err(|e| Error::Internal(format!("Error fetching delegated stake. {e}")))?;

        let Some(mut delegated_stake) = delegated_stakes.pop() else {
            return Err(Error::Internal(
                "Error fetching delegated stake. No pools returned.".to_string(),
            ));
        };

        let Some(stake) = delegated_stake.stakes.pop() else {
            return Err(Error::Internal(
                "Error fetching delegated stake. No stake in pool.".to_string(),
            ));
        };

        Ok(stake)
    }
}

/// `checkpoint_viewed_at` represents the checkpoint sequence number at which the set of
/// `SuiValidatorSummary` was queried for. Each `Validator` will inherit this checkpoint, so that
/// when viewing the `Validator`'s state, it will be as if it was read at the same checkpoint.
pub(crate) fn convert_to_validators(
    validators: Vec<SuiValidatorSummary>,
    system_state: Option<NativeSuiSystemStateSummary>,
    checkpoint_viewed_at: u64,
) -> Vec<Validator> {
    let (at_risk, reports) = if let Some(NativeSuiSystemStateSummary {
        at_risk_validators,
        validator_report_records,
        ..
    }) = system_state
    {
        (
            BTreeMap::from_iter(at_risk_validators),
            BTreeMap::from_iter(validator_report_records),
        )
    } else {
        Default::default()
    };

    validators
        .into_iter()
        .map(|validator_summary| {
            let at_risk = at_risk.get(&validator_summary.sui_address).copied();
            let report_records = reports.get(&validator_summary.sui_address).map(|addrs| {
                addrs
                    .iter()
                    .cloned()
                    .map(|a| Address {
                        address: SuiAddress::from(a),
                        checkpoint_viewed_at: Some(checkpoint_viewed_at),
                    })
                    .collect()
            });

            Validator {
                validator_summary,
                at_risk,
                report_records,
                checkpoint_viewed_at,
            }
        })
        .collect()
}
