// Copyright (c) 2022, Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use crate::{authority::SuiDataStore, gateway_types::TransactionEffectsResponse};
use anyhow::anyhow;
use serde::{Deserialize, Serialize};
use sui_types::{base_types::*, batch::TxSequenceNumber, error::SuiError, fp_ensure};
use tracing::debug;

const MAX_TX_RANGE_SIZE: u64 = 4096;

pub struct QueryHelpers<const ALL_OBJ_VER: bool, const USE_LOCKS: bool, S> {
    _s: std::marker::PhantomData<S>,
}

impl<
        const ALL_OBJ_VER: bool,
        const USE_LOCKS: bool,
        S: Eq + Serialize + for<'de> Deserialize<'de>,
    > QueryHelpers<ALL_OBJ_VER, USE_LOCKS, S>
{
    pub fn get_total_transaction_number(
        database: &SuiDataStore<ALL_OBJ_VER, USE_LOCKS, S>,
    ) -> Result<u64, anyhow::Error> {
        Ok(database.next_sequence_number()?)
    }

    pub fn get_transactions_in_range(
        database: &SuiDataStore<ALL_OBJ_VER, USE_LOCKS, S>,
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
        database: &SuiDataStore<ALL_OBJ_VER, USE_LOCKS, S>,
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
        database: &SuiDataStore<ALL_OBJ_VER, USE_LOCKS, S>,
        digest: TransactionDigest,
    ) -> Result<TransactionEffectsResponse, anyhow::Error> {
        let opt = database.get_certified_transaction(&digest)?;
        match opt {
            Some(certificate) => Ok(TransactionEffectsResponse {
                certificate: certificate.try_into()?,
                effects: database.get_effects(&digest)?.into(),
            }),
            None => Err(anyhow!(SuiError::TransactionNotFound { digest })),
        }
    }
}
