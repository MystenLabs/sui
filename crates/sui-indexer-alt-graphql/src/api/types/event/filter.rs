// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use std::ops::Range;

use anyhow::Context as _;
use async_graphql::InputObject;

use sui_pg_db::query::Query;
use sui_sql_macro::query;
use sui_types::event::Event as NativeEvent;

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
    pub digest: Option<Digest>,

    /// Filter on events by transaction sender address.
    pub sender: Option<SuiAddress>,

    /// Events emitted by a particular module. An event is emitted by a
    /// particular module if some function in the module is called by a
    /// PTB and emits an event.
    ///
    /// Modules can be filtered by their package, or package::module.
    /// We currently do not support filtering by emitting module and event type
    /// at the same time so if both are provided in one filter, the query will error.
    pub module: Option<ModuleFilter>,

    /// This field is used to specify the type of event emitted.
    ///
    /// Events can be filtered by their type's package, package::module,
    /// or their fully qualified type name.
    ///
    /// Generic types can be queried by either the generic type name, e.g.
    /// `0x2::coin::Coin`, or by the full type name, such as
    /// `0x2::coin::Coin<0x2::sui::SUI>`.
    pub type_: Option<TypeFilter>,
}

impl EventFilter {
    /// Builds a SQL query to select and filter events based on sender, module, and type filters.
    /// Uses the provided transaction bounds subquery to limit results to a specific transaction range
    pub(crate) fn query(&self, tx_bounds_subquery: Query<'static>) -> Result<Query, RpcError> {
        let sender_filter = if let Some(sender) = self.sender {
            query!("AND sender = {Bytea}", sender.into_vec())
        } else {
            query!("")
        };

        let query = match (self.sender, &self.module, &self.type_) {
            (_, None, None) => {
                // No type or module filter - just use tx_bounds and sender
                query!(
                    r#"
                    WITH bounds AS ({})
                    SELECT tx_sequence_number FROM ev_struct_inst, bounds
                    WHERE tx_sequence_number >= bounds.tx_lo 
                    AND tx_sequence_number < bounds.tx_hi
                    {}
                    "#,
                    tx_bounds_subquery,
                    sender_filter
                )
            }
            (_, None, Some(event_type)) => {
                // Event type filter
                match event_type {
                    TypeFilter::Package(package) => {
                        query!(
                            r#"
                            WITH bounds AS ({})
                            SELECT tx_sequence_number FROM ev_struct_inst, bounds
                            WHERE package = {Bytea} 
                            AND tx_sequence_number >= bounds.tx_lo 
                            AND tx_sequence_number < bounds.tx_hi
                            {}
                            "#,
                            tx_bounds_subquery,
                            package.into_vec(),
                            sender_filter
                        )
                    }
                    TypeFilter::Module(package, module) => {
                        query!(
                            r#"
                            WITH bounds AS ({})
                            SELECT tx_sequence_number FROM ev_struct_inst, bounds
                            WHERE package = {Bytea} AND module = {Text} 
                            AND tx_sequence_number >= bounds.tx_lo 
                            AND tx_sequence_number < bounds.tx_hi
                            {}
                            "#,
                            tx_bounds_subquery,
                            package.into_vec(),
                            module,
                            sender_filter
                        )
                    }
                    TypeFilter::Type(tag) => {
                        let package = tag.address.to_vec();
                        let module = tag.module.to_string();
                        let name = tag.name.as_str().to_owned();
                        let type_params_bytes = bcs::to_bytes(&tag.type_params)
                            .context("Failed to serialize type parameters")?;

                        query!(
                            r#"
                            WITH bounds AS ({})
                            SELECT tx_sequence_number FROM ev_struct_inst, bounds
                            WHERE 
                            package = {Bytea} AND module = {Text} AND name = {Text} AND instantiation = {Bytea} 
                            AND tx_sequence_number >= bounds.tx_lo 
                            AND tx_sequence_number < bounds.tx_hi
                            {}
                            "#,
                            tx_bounds_subquery,
                            package,
                            module,
                            name,
                            type_params_bytes,
                            sender_filter
                        )
                    }
                }
            }
            (_, Some(module), None) => {
                // Module filter
                match module {
                    ModuleFilter::Package(package) => {
                        query!(
                            r#"
                            WITH bounds AS ({})
                            SELECT tx_sequence_number FROM ev_emit_mod, bounds
                            WHERE package = {Bytea} 
                            AND tx_sequence_number >= bounds.tx_lo 
                            AND tx_sequence_number < bounds.tx_hi
                            {}
                            "#,
                            tx_bounds_subquery,
                            package.into_vec(),
                            sender_filter
                        )
                    }
                    ModuleFilter::Module(package, module) => {
                        query!(
                            r#"
                            WITH bounds AS ({})
                            SELECT tx_sequence_number FROM ev_emit_mod, bounds
                            WHERE package = {Bytea} AND module = {Text} 
                            AND tx_sequence_number >= bounds.tx_lo 
                            AND tx_sequence_number < bounds.tx_hi
                            {}
                            "#,
                            tx_bounds_subquery,
                            package.into_vec(),
                            module,
                            sender_filter
                        )
                    }
                }
            }
            (_, Some(_), Some(_)) => {
                return Err(feature_unavailable(
                    "Filtering by both emitting module and event type is not supported",
                ))
            }
        };

        Ok(query)
    }

    // Check if the Event matches sender, module, or type filters in EventFilter if they are provided.
    pub(crate) fn matches(&self, event: &NativeEvent) -> bool {
        let sender_matches = |sender: &Option<SuiAddress>| {
            sender
                .as_ref()
                .is_none_or(|s| s == &SuiAddress::from(event.sender))
        };

        match (self.sender, &self.module, &self.type_) {
            (Some(sender), None, None) => sender == SuiAddress::from(event.sender),
            (sender, Some(module), None) => {
                sender_matches(&sender)
                    && match module {
                        ModuleFilter::Package(package) => {
                            SuiAddress::from(event.package_id) == *package
                        }
                        ModuleFilter::Module(package, module_name) => {
                            SuiAddress::from(event.package_id) == *package
                                && event.transaction_module.as_str() == module_name
                        }
                    }
            }
            (sender, None, Some(event_type)) => {
                sender_matches(&sender)
                    && match event_type {
                        TypeFilter::Package(package) => {
                            SuiAddress::from(event.type_.address) == *package
                        }
                        TypeFilter::Module(package, module_name) => {
                            SuiAddress::from(event.type_.address) == *package
                                && event.type_.module.as_str() == module_name
                        }
                        TypeFilter::Type(tag) => {
                            if tag.type_params.is_empty() {
                                tag.address == event.type_.address
                                    && tag.module == event.type_.module
                                    && tag.name == event.type_.name
                            } else {
                                tag == &event.type_
                            }
                        }
                    }
            }
            (_, Some(_), Some(_)) => false,
            (None, None, None) => true,
        }
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
