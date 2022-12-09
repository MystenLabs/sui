// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use crate::authority::AuthorityStore;
use anyhow::anyhow;
use sui_types::messages::{TransactionEffects, VerifiedCertificate};
use sui_types::{base_types::*, batch::TxSequenceNumber, error::SuiError, fp_ensure};
use tracing::debug;

pub const MAX_TX_RANGE_SIZE: u64 = 4096;

pub struct QueryHelpers {}

impl QueryHelpers {
    pub fn get_total_transaction_number(database: &AuthorityStore) -> Result<u64, anyhow::Error> {
        Ok(database.next_sequence_number()?)
    }

    pub fn get_transactions_in_range(
        database: &AuthorityStore,
        start: TxSequenceNumber,
        end: TxSequenceNumber,
    ) -> Result<Vec<(TxSequenceNumber, TransactionDigest)>, anyhow::Error> {
        fp_ensure!(
            start <= end,
            SuiError::FullNodeInvalidTxRangeQuery {
                error: format!(
                    "start must not exceed end, (start={}, end={}) given",
                    start, end
                ),
            }
            .into()
        );
        fp_ensure!(
            end - start <= MAX_TX_RANGE_SIZE,
            SuiError::FullNodeInvalidTxRangeQuery {
                error: format!(
                    "Number of transactions queried must not exceed {}, {} queried",
                    MAX_TX_RANGE_SIZE,
                    end - start
                ),
            }
            .into()
        );
        let res = database
            .transactions_in_seq_range(start, end)?
            .into_iter()
            .map(|(seq, digests)| (seq, digests.transaction))
            .collect();
        debug!(?start, ?end, ?res, "Fetched transactions");
        Ok(res)
    }

    pub fn get_recent_transactions(
        database: &AuthorityStore,
        count: u64,
    ) -> Result<Vec<(TxSequenceNumber, TransactionDigest)>, anyhow::Error> {
        fp_ensure!(
            count <= MAX_TX_RANGE_SIZE,
            SuiError::FullNodeInvalidTxRangeQuery {
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
        database: &AuthorityStore,
        digest: &TransactionDigest,
    ) -> Result<(VerifiedCertificate, TransactionEffects), anyhow::Error> {
        let opt = database.get_certified_transaction(digest)?;
        match opt {
            Some(certificate) => Ok((certificate, database.get_effects(digest)?)),
            None => Err(anyhow!(SuiError::TransactionNotFound { digest: *digest })),
        }
    }
}
