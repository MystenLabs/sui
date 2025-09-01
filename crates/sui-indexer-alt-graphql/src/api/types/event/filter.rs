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
        module_filter::ModuleFilter, sui_address::SuiAddress, type_filter::TypeFilter,
        uint53::UInt53,
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

    /// Filter down to the events from this transaction (given by its transaction digest).
    // TODO: (henry) Implement these filters.
    // pub transaction_digest: Option<Digest>,

    /// Filter down to events from transactions sent by this address.
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
    pub(crate) fn query(&self, tx_bounds: std::ops::Range<u64>) -> Result<Query, RpcError> {
        match (self.sender, &self.module, &self.type_) {
            (sender, None, None) => Ok(select_tx_sequence_numbers(tx_bounds, sender)),
            (sender, None, Some(event_type)) => {
                Ok(select_event_type(event_type, tx_bounds, sender)?)
            }
            (sender, Some(module), None) => Ok(select_emit_module(module, tx_bounds, sender)),
            (_, Some(_), Some(_)) => {
                return Err(feature_unavailable(
                    "Filtering by both emitting module and event type is not supported",
                ))
            }
        }
    }

    // Check if the Event matches sender, module, or type filters in EventFilter if they are provided.
    pub(crate) fn matches(&self, event: &NativeEvent) -> bool {
        let sender_matches = |sender: &Option<SuiAddress>| {
            sender
                .as_ref()
                .map_or(true, |s| s == &SuiAddress::from(event.sender))
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

fn select_tx_sequence_numbers(
    tx_bounds: std::ops::Range<u64>,
    sender: Option<SuiAddress>,
) -> Query<'static> {
    let where_clause = if let Some(sender) = sender {
        query!("AND sender = {Bytea}", sender.into_vec())
    } else {
        query!("")
    };
    let query = query!(
        r#"
        SELECT tx_sequence_number FROM ev_struct_inst
        WHERE tx_sequence_number >= {BigInt} AND tx_sequence_number < {BigInt}
        {}
        "#,
        tx_bounds.start as i64,
        tx_bounds.end as i64,
        where_clause
    );
    query
}

fn select_event_type(
    event_type: &TypeFilter,
    tx_bounds: std::ops::Range<u64>,
    sender: Option<SuiAddress>,
) -> Result<Query<'_>, RpcError> {
    match event_type {
        TypeFilter::Package(package) => {
            let query = query!(
                r#"
                SELECT tx_sequence_number FROM ev_struct_inst WHERE package = {Bytea} AND tx_sequence_number >= {BigInt} AND tx_sequence_number < {BigInt} {}
                "#,
                package.into_vec(),
                tx_bounds.start as i64,
                tx_bounds.end as i64,
                if let Some(sender) = sender {
                    query!("AND sender = {Bytea}", sender.into_vec())
                } else {
                    query!("")
                }
            );
            Ok(query)
        }
        TypeFilter::Module(package, module) => {
            let query = query!(
                r#"
                SELECT tx_sequence_number FROM ev_struct_inst WHERE package = {Bytea} AND module = {Text} AND tx_sequence_number >= {BigInt} AND tx_sequence_number < {BigInt} {}
                "#,
                package.into_vec(),
                module,
                tx_bounds.start as i64,
                tx_bounds.end as i64,
                if let Some(sender) = sender {
                    query!("AND sender = {Bytea}", sender.into_vec())
                } else {
                    query!("")
                }
            );
            Ok(query)
        }
        TypeFilter::Type(tag) => {
            let package = tag.address.to_vec();
            let module = tag.module.to_string();
            let name = tag.name.as_str().to_owned();
            let type_params_bytes =
                bcs::to_bytes(&tag.type_params).context("Failed to serialize type parameters")?;

            let query = query!(
                r#"
                SELECT tx_sequence_number FROM ev_struct_inst 
                WHERE 
                package = {Bytea} AND module = {Text} AND name = {Text} AND instantiation = {Bytea} AND tx_sequence_number >= {BigInt} AND tx_sequence_number < {BigInt}
                {}
                "#,
                package,
                module,
                name,
                type_params_bytes,
                tx_bounds.start as i64,
                tx_bounds.end as i64,
                if let Some(sender) = sender {
                    query!("AND sender = {Bytea}", sender.into_vec())
                } else {
                    query!("")
                },
            );
            Ok(query)
        }
    }
}

fn select_emit_module(
    module: &ModuleFilter,
    tx_bounds: std::ops::Range<u64>,
    sender: Option<SuiAddress>,
) -> Query<'_> {
    match module {
        ModuleFilter::Package(package) => {
            let query = query!(
                r#"
                SELECT tx_sequence_number FROM ev_emit_mod WHERE package = {Bytea} AND tx_sequence_number >= {BigInt} AND tx_sequence_number < {BigInt}
                {}
                "#,
                package.into_vec(),
                tx_bounds.start as i64,
                tx_bounds.end as i64,
                if let Some(sender) = sender {
                    query!("AND sender = {Bytea}", sender.into_vec())
                } else {
                    query!("")
                }
            );
            query
        }
        ModuleFilter::Module(package, module) => {
            let query = query!(
                r#"
                SELECT tx_sequence_number FROM ev_emit_mod WHERE package = {Bytea} AND module = {Text} AND tx_sequence_number >= {BigInt} AND tx_sequence_number < {BigInt}
                {}
                "#,
                package.into_vec(),
                module,
                tx_bounds.start as i64,
                tx_bounds.end as i64,
                if let Some(sender) = sender {
                    query!("AND sender = {Bytea}", sender.into_vec())
                } else {
                    query!("")
                }
            );
            query
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
