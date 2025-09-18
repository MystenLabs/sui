// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use std::ops::Range;

use anyhow::Context as _;
use async_graphql::InputObject;
use sui_pg_db::query::Query;
use sui_sql_macro::query;
use sui_types::event::Event as NativeEvent;

use crate::{
    api::{
        scalars::{
            module_filter::ModuleFilter, sui_address::SuiAddress, type_filter::TypeFilter,
            uint53::UInt53,
        },
        types::lookups::CheckpointBounds,
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
    /// Builds a SQL query to select and filter events based on sender, module, and type filters.
    /// Uses the provided transaction bounds subquery to limit results to a specific transaction range
    pub(crate) fn query<'q>(&self) -> Result<Query<'q>, RpcError> {
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
            SELECT
                tx_sequence_number
            FROM
                {}
            WHERE
                tx_sequence_number >= (SELECT tx_lo FROM tx_lo)
                AND tx_sequence_number < (SELECT tx_hi FROM tx_hi)
            "#,
            table,
        );

        if let Some(sender) = self.sender {
            query += query!(" AND sender = {Bytea}", sender.into_vec());
        }

        if let Some(module) = &self.module {
            if let Some(package) = module.package() {
                query += query!(" AND package = {Bytea}", package.into_vec());
            }
            if let Some(module) = module.module() {
                query += query!(" AND module = {Text}", module.to_string());
            }
        }

        if let Some(type_) = &self.type_ {
            if let Some(package) = type_.package() {
                query += query!(" AND package = {Bytea}", package.into_vec());
            }

            if let Some(module) = type_.module() {
                query += query!(" AND module = {Text}", module.to_string());
            }

            if let Some(type_name) = type_.type_name() {
                query += query!(" AND name = {Text}", type_name.to_string());
            }

            if let Some(type_params) = type_.type_params() {
                if !type_params.is_empty() {
                    query += query!(
                        " AND instantiation = {Bytea}",
                        bcs::to_bytes(&type_params)
                            .context("Failed to serialize type parameters")?
                    );
                }
            }
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

        if let Some(module) = &self.module {
            if let Some(package) = module.package() {
                if package != SuiAddress::from(event.package_id) {
                    return false;
                }
            }
            if let Some(module) = module.module() {
                if module != event.transaction_module.as_str() {
                    return false;
                }
            }
        }

        if let Some(type_) = &self.type_ {
            if let Some(package) = type_.package() {
                if package != SuiAddress::from(event.type_.address) {
                    return false;
                }
            }
            if let Some(module) = type_.module() {
                if module != event.type_.module.as_str() {
                    return false;
                }
            }
            if let Some(type_name) = type_.type_name() {
                if type_name != event.type_.name.as_str() {
                    return false;
                }
            }
            if type_
                .type_params()
                .is_some_and(|p| p != event.type_.type_params.as_slice())
            {
                return false;
            }
        }

        true
    }
}

impl CheckpointBounds for EventFilter {
    fn after_checkpoint(&self) -> Option<UInt53> {
        self.after_checkpoint
    }

    fn at_checkpoint(&self) -> Option<UInt53> {
        self.at_checkpoint
    }

    fn before_checkpoint(&self) -> Option<UInt53> {
        self.before_checkpoint
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
