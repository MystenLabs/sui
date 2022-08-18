// Copyright (c) 2022, Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use crate::authority::SuiDataStore;
use anyhow::anyhow;
use serde::{Deserialize, Serialize};
use std::fmt::Debug;
use sui_types::crypto::AuthoritySignInfoTrait;
use sui_types::messages::{CertifiedTransaction, TransactionEffects};
use sui_types::{base_types::*, batch::TxSequenceNumber, error::SuiError, fp_ensure};
use tracing::debug;

const MAX_TX_RANGE_SIZE: u64 = 4096;

pub struct QueryHelpers<S> {
    _s: std::marker::PhantomData<S>,
}

// TODO: QueryHelpers contains query implementations for the Gateway read API that would otherwise
// be duplicated between AuthorityState and GatewayState. The gateway read API will be removed
// soon, since nodes will be handling that. At that point we should delete this struct and move the
// code back to AuthorityState.
impl<S: Eq + Debug + Serialize + for<'de> Deserialize<'de> + AuthoritySignInfoTrait>
    QueryHelpers<S>
{
    pub fn get_total_transaction_number(database: &SuiDataStore<S>) -> Result<u64, anyhow::Error> {
        Ok(database.next_sequence_number()?)
    }

    pub fn get_transactions_in_range(
        database: &SuiDataStore<S>,
        start: TxSequenceNumber,
        end: TxSequenceNumber,
    ) -> Result<Vec<(TxSequenceNumber, TransactionDigest)>, anyhow::Error> {
        fp_ensure!(
            start <= end,
            SuiError::GatewayInvalidTxRangeQuery {
                error: format!(
                    "start must not exceed end, (start={}, end={}) given",
                    start, end
                ),
            }
            .into()
        );
        fp_ensure!(
            end - start <= MAX_TX_RANGE_SIZE,
            SuiError::GatewayInvalidTxRangeQuery {
                error: format!(
                    "Number of transactions queried must not exceed {}, {} queried",
                    MAX_TX_RANGE_SIZE,
                    end - start
                ),
            }
            .into()
        );
        let res = database.transactions_in_seq_range(start, end)?;
        debug!(?start, ?end, ?res, "Fetched transactions");
        Ok(res)
    }

    pub fn get_recent_transactions(
        database: &SuiDataStore<S>,
        count: u64,
    ) -> Result<Vec<(TxSequenceNumber, TransactionDigest)>, anyhow::Error> {
        fp_ensure!(
            count <= MAX_TX_RANGE_SIZE,
            SuiError::GatewayInvalidTxRangeQuery {
                error: format!(
                    "Number of transactions queried must not exceed {}, {} queried",
                    MAX_TX_RANGE_SIZE, count
                ),
            }
            .into()
        );
        let end = Self::get_total_transaction_number(database)?;
        let start = if end >= count { end - count } else { 0 };
        Self::get_transactions_in_range(database, start, end)
    }

    pub fn get_transaction(
        database: &SuiDataStore<S>,
        digest: &TransactionDigest,
    ) -> Result<(CertifiedTransaction, TransactionEffects), anyhow::Error> {
        let opt = database.get_certified_transaction(digest)?;
        match opt {
            Some(certificate) => Ok((certificate, database.get_effects(digest)?)),
            None => Err(anyhow!(SuiError::TransactionNotFound { digest: *digest })),
        }
    }
}
