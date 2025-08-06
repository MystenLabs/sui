// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0
use anyhow::Context as _;

use async_graphql::{Context, InputObject};
use diesel::prelude::QueryableByName;
use diesel::sql_types::BigInt;
use std::ops::RangeInclusive;
use sui_indexer_alt_reader::pg_reader::PgReader;
use sui_sql_macro::query;

use crate::{
    api::{
        scalars::{sui_address::SuiAddress, uint53::UInt53},
        types::transaction::CTransaction,
    },
    error::RpcError,
    intersect,
    pagination::Page,
};

#[derive(InputObject, Debug, Default, Clone)]
pub(crate) struct TransactionFilter {
    /// Limit to transactions that occured strictly after the given checkpoint.
    pub after_checkpoint: Option<UInt53>,

    /// Limit to transactions in the given checkpoint.
    pub at_checkpoint: Option<UInt53>,

    /// Limit to transaction that occured strictly before the given checkpoint.
    pub before_checkpoint: Option<UInt53>,

    /// Limit to transactions that interacted with the given address. The address could be a
    /// sender, sponsor, or recipient of the transaction.
    pub affected_address: Option<SuiAddress>,

    /// Limit to transactions that were sent by the given address.
    pub sender: Option<SuiAddress>,
}

impl TransactionFilter {
    /// Try to create a filter whose results are the intersection of transaction blocks in `self`'s
    /// results and transaction blocks in `other`'s results. This may not be possible if the
    /// resulting filter is inconsistent in some way (e.g. a filter that requires one field to be
    /// two different values simultaneously).
    pub(crate) fn intersect(self, other: Self) -> Option<Self> {
        macro_rules! intersect {
            ($field:ident, $body:expr) => {
                intersect::field(self.$field, other.$field, $body)
            };
        }

        Some(Self {
            after_checkpoint: intersect!(after_checkpoint, intersect::by_max)?,
            at_checkpoint: intersect!(at_checkpoint, intersect::by_eq)?,
            before_checkpoint: intersect!(before_checkpoint, intersect::by_min)?,
            affected_address: intersect!(affected_address, intersect::by_eq)?,
            sender: intersect!(sender, intersect::by_eq)?,
        })
    }

    /// A TransactionBlockFilter is considered not to have any filters if no filters are specified,
    /// or if the only filters are on `checkpoint`.
    pub(crate) fn has_filters(&self) -> bool {
        // TODO: Add more filters here, for now we only have filters on address.
        self.affected_address.is_some() || self.sender.is_some()
    }

    pub(crate) fn has_address_filters(&self) -> bool {
        self.affected_address.is_some() || self.sender.is_some()
    }
}

#[derive(QueryableByName)]
struct TxSequenceNumbers {
    #[diesel(sql_type = BigInt, column_name = "tx_sequence_number")]
    tx_sequence_number: i64,
}

/// Transaction sequence numbers optionally filtered by sender address and affected address
/// within specified checkpoint bounds.
pub(super) async fn tx_by_address(
    ctx: &Context<'_>,
    cp_bounds: &RangeInclusive<u64>,
    affected_address: Option<SuiAddress>,
    sender: Option<SuiAddress>,
    page: &Page<CTransaction>,
    global_tx_hi: u64,
) -> Result<Vec<u64>, RpcError> {
    let pg_reader: &PgReader = ctx.data()?;

    let mut address_conditions = query!("");
    if let Some(affected_address) = affected_address {
        address_conditions += query!(" AND taa.affected = {Bytea} ", affected_address.into_vec())
    }
    if let Some(sender) = sender {
        address_conditions += query!(" AND taa.sender = {Bytea} ", sender.into_vec())
    }

    let mut pagination = query!("");
    if let Some(after) = page.after() {
        pagination += query!(" AND taa.tx_sequence_number >= {BigInt} ", **after as i64)
    }
    if let Some(before) = page.before() {
        pagination += query!(" AND taa.tx_sequence_number <= {BigInt} ", **before as i64)
    }

    let query = query!(
        r#"
        WITH checkpoint_bounds AS (
            SELECT 
                cp_start.tx_lo,
                COALESCE(
                    (
                    SELECT tx_lo FROM cp_sequence_numbers 
                    WHERE cp_sequence_number = {BigInt} + 1 
                    LIMIT 1
                    ),
                    {BigInt}
                ) as tx_hi
            FROM cp_sequence_numbers cp_start
            WHERE cp_start.cp_sequence_number = {BigInt}
        )
        SELECT taa.tx_sequence_number
        FROM tx_affected_addresses taa
        CROSS JOIN checkpoint_bounds cb
        WHERE taa.tx_sequence_number >= cb.tx_lo
            AND taa.tx_sequence_number < cb.tx_hi
            {}
            {}
        ORDER BY {}
        LIMIT {BigInt}
        "#,
        *cp_bounds.end() as i64,
        global_tx_hi as i64,
        *cp_bounds.start() as i64,
        pagination,
        address_conditions,
        if page.is_from_front() {
            query!("taa.tx_sequence_number")
        } else {
            query!("taa.tx_sequence_number DESC")
        },
        (page.limit() + 2) as i64,
    );

    let mut conn = pg_reader
        .connect()
        .await
        .context("Failed to connect to database")?;

    let mut results: Vec<TxSequenceNumbers> = conn
        .results(query)
        .await
        .context("Failed to execute query")?;

    if !page.is_from_front() {
        results.reverse();
    }

    Ok(results
        .into_iter()
        .map(|x| x.tx_sequence_number as u64)
        .collect())
}
