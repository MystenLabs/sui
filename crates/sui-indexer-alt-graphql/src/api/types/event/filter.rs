// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use std::ops::Range;

use anyhow::Context as _;
use async_graphql::InputObject;

use sui_pg_db::query::Query;
use sui_sql_macro::query;
use sui_types::{event::Event as NativeEvent, TypeTag};

use crate::{
    api::scalars::{
        digest::Digest, module_filter::ModuleFilter, sui_address::SuiAddress,
        type_filter::TypeFilter, uint53::UInt53,
    },
    error::{feature_unavailable, RpcError},
    pagination::Page,
};

use super::CEvent;

#[derive(InputObject, Debug, Default, Clone)]
pub(crate) struct EventFilter {
    /// Limit to events that occured strictly after the given checkpoint.
    pub after_checkpoint: Option<UInt53>,

    /// Limit to events in the given checkpoint.
    pub at_checkpoint: Option<UInt53>,

    /// Limit to event that occured strictly before the given checkpoint.
    pub before_checkpoint: Option<UInt53>,

    /// Filter on events by transaction digest.
    pub transaction_digest: Option<Digest>,

    /// Filter on events by transaction sender address.
    pub sender: Option<SuiAddress>,

    /// Events emitted by a particular module. An event is emitted by a particular module if some function in the module is called by a PTB and emits an event.
    ///
    /// Modules can be filtered by their package, or package::module. We currently do not support filtering by emitting module and event type at the same time so if both are provided in one filter, the query will error.
    pub module: Option<ModuleFilter>,

    /// This field is used to specify the type of event emitted.
    ///
    /// Events can be filtered by their type's package, package::module, or their fully qualified type name.
    ///
    /// Generic types can be queried by either the generic type name, e.g. `0x2::coin::Coin`, or by the full type name, such as `0x2::coin::Coin<0x2::sui::SUI>`.
    pub type_: Option<TypeFilter>,
}

impl EventFilter {
    pub(crate) fn package(&self) -> Option<SuiAddress> {
        self.module
            .as_ref()
            .and_then(|m| m.package())
            .or_else(|| self.type_.as_ref().and_then(|t| t.package()))
    }

    pub(crate) fn module(&self) -> Option<String> {
        self.module
            .as_ref()
            .and_then(|m| m.module())
            .or_else(|| self.type_.as_ref().and_then(|t| t.module()))
    }

    pub(crate) fn type_name(&self) -> Option<String> {
        self.type_.as_ref().and_then(|t| t.type_name())
    }
    pub(crate) fn type_params(&self) -> Option<Vec<TypeTag>> {
        self.type_.as_ref().and_then(|t| t.type_params())
    }

    /// Builds a SQL query to select and filter events based on sender, module, and type filters.
    /// Uses the provided transaction bounds subquery to limit results to a specific transaction range
    pub(crate) fn query<'q>(&self, tx_bounds_subquery: Query<'q>) -> Result<Query<'q>, RpcError> {
        let table = match (&self.module, &self.type_) {
            (Some(_), Some(_)) => {
                return Err(feature_unavailable(
                    "Filtering by both emitting module and event type is not supported",
                ))
            }
            (Some(_), None) => query!("ev_emit_mod"),
            (None, _) => query!("ev_struct_inst"),
        };

        let mut query = query!(
            r#"
            WITH bounds AS ({})
            SELECT
                tx_sequence_number
            FROM
                bounds,
                {}
            WHERE
                tx_sequence_number >= bounds.tx_lo
                AND tx_sequence_number < bounds.tx_hi
            "#,
            tx_bounds_subquery,
            table
        );

        if let Some(sender) = self.sender {
            query += query!(" AND sender = {Bytea}", sender.into_vec());
        }

        if let Some(package) = self.package() {
            query += query!(" AND package = {Bytea}", package.into_vec());
        }

        if let Some(module) = self.module() {
            query += query!(" AND module = {Text}", module);
        }

        if let Some(type_name) = self.type_name() {
            query += query!(" AND name = {Text}", type_name);
        }

        if let Some(type_params) = self.type_params() {
            if !type_params.is_empty() {
                query += query!(
                    " AND instantiation = {Bytea}",
                    bcs::to_bytes(&type_params).context("Failed to serialize type parameters")?
                );
            }
        }

        if let Some(digest) = self.transaction_digest {
            query += query!(" AND tx_sequence_number = (SELECT tx_sequence_number FROM tx_digests WHERE tx_digest = {Bytea})", digest.into_inner());
        }

        Ok(query)
    }

    // Check if the Event matches sender, module, or type filters in EventFilter if they are provided.
    pub(crate) fn matches(&self, event: &NativeEvent) -> bool {
        if self
            .sender
            .is_some_and(|s| s != SuiAddress::from(event.sender))
        {
            return false;
        }

        if self
            .package()
            .is_some_and(|p| p != SuiAddress::from(event.package_id))
        {
            return false;
        }

        if self
            .module()
            .is_some_and(|m| m != event.transaction_module.to_string())
        {
            return false;
        }
        if let Some(type_name) = self.type_name() {
            if type_name != event.type_.name.to_string() {
                return false;
            }
        }

        if self
            .type_params()
            .is_some_and(|p| !p.is_empty() && p != event.type_.type_params.to_vec())
        {
            return false;
        }

        true
    }
}

/// The event indices (sequence_number) in a transaction's events array that are within the cursor bounds, inclusively.
/// Event transaction numbers are always returned in ascending order.
pub(super) fn tx_ev_bounds(
    page: &Page<CEvent>,
    tx_sequence_number: u64,
    event_count: usize,
) -> Range<usize> {
    // Find start index from 'after' cursor, defaults to 0
    let ev_lo = page
        .after()
        .filter(|c| c.tx_sequence_number == tx_sequence_number)
        .map(|c| c.ev_sequence_number as usize)
        .unwrap_or(0)
        .min(event_count);

    // Find exclusive end index from 'before' cursor, default to event_count
    let ev_hi = page
        .before()
        .filter(|c| c.tx_sequence_number == tx_sequence_number)
        .map(|c| (c.ev_sequence_number as usize).saturating_add(1))
        .unwrap_or(event_count)
        .max(ev_lo)
        .min(event_count);

    ev_lo..ev_hi
}
