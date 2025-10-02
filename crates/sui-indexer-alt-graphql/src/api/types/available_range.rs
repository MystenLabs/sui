// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use async_graphql::{Context, Object};
use std::{collections::BTreeSet, sync::Arc};

use crate::{error::RpcError, scope::Scope, task::watermark::Watermarks};

use super::checkpoint::Checkpoint;

/// Identifies a GraphQL query component that is used to determine the range of checkpoints for which data is available (for data that can be tied to a particular checkpoint)
///
/// Both `type_` and `field` are required. The `filter` is optional and provides retention information for filtered queries.
#[derive(Debug, Clone, Eq, PartialEq)]
pub(crate) struct AvailableRangeKey {
    /// The GraphQL type to check retention for
    pub(crate) type_: String,

    /// The specific field within the type to check retention for
    pub(crate) field: Option<String>,

    /// Optional filter to check retention for filtered queries
    pub(crate) filters: Option<Vec<String>>,
}

#[derive(Clone)]
pub struct AvailableRange {
    pub scope: Scope,
    pub first: u64,
}

/// Checkpoint range for which data is available.
#[Object]
impl AvailableRange {
    /// Inclusive lower checkpoint for which data is available.
    async fn first(&self) -> Result<Option<Checkpoint>, RpcError> {
        Ok(Checkpoint::with_sequence_number(
            self.scope.clone(),
            Some(self.first),
        ))
    }

    /// Inclusive upper checkpoint for which data is available.
    async fn last(&self) -> Result<Option<Checkpoint>, RpcError> {
        Ok(Checkpoint::with_sequence_number(self.scope.clone(), None))
    }
}

impl AvailableRange {
    /// Get retention information for a specific query type and field
    pub(crate) fn new(
        ctx: &Context<'_>,
        scope: &Scope,
        retention_key: AvailableRangeKey,
    ) -> Result<Self, RpcError> {
        let watermarks: &Arc<Watermarks> = ctx.data()?;
        let mut pipelines = BTreeSet::new();
        collect_pipelines(
            &retention_key.type_,
            retention_key.field.as_deref(),
            BTreeSet::from_iter(retention_key.filters.unwrap_or_default()),
            &mut pipelines,
        );

        let first =
            pipelines
                .iter()
                .try_fold(0, |acc: u64, pipeline| -> Result<u64, RpcError> {
                    let watermark = watermarks.pipeline_lo_watermark(pipeline)?;
                    let checkpoint = watermark.checkpoint();
                    Ok(acc.max(checkpoint))
                })?;

        Ok(Self {
            scope: scope.clone(),
            first,
        })
    }
}

/// Maps GraphQL query components to watermark pipeline names.
///
/// Determines which watermark pipelines are relevant for a given GraphQL query.
/// The pipeline names are used to query watermark data to determine the
/// checkpoint sequence range (available range) for which data is available.
///
fn collect_pipelines(
    type_: &str,
    field: Option<&str>,
    filters: BTreeSet<String>,
    pipelines: &mut BTreeSet<String>,
) {
    match (type_, field, filters) {
        // Address fields
        ("Address", Some("transactions"), mut filters) => {
            filters.insert("affectedAddress".to_string());
            collect_pipelines("Query", Some("transactions"), filters, pipelines);
        }
        (
            "Address",
            field @ Some("balance" | "balances" | "multiGetBalances" | "objects"),
            filters,
        ) => {
            collect_pipelines("IAddressable", field, filters, pipelines);
        }
        // An Address can access MoveObjects by getting the object at its address. But IAddressable does
        // not implement IMoveObject because it will incorrectly require MovePackage to implement IMoveObject.
        //
        ("Address", Some("dynamicFields"), filters) => {
            collect_pipelines("IMoveObject", Some("dynamicFields"), filters, pipelines);
        }

        // Checkpoint fields
        ("Checkpoint", Some("transactions"), mut filters) => {
            filters.insert("atCheckpoint".to_string());
            collect_pipelines("Query", Some("transactions"), filters, pipelines);
        }

        // CoinMetadata fields
        ("CoinMetadata", filter @ Some("balance" | "balances" | "multiGetBalances"), filters) => {
            collect_pipelines("IAddressable", filter, filters, pipelines);
        }
        ("CoinMetadata", Some("dynamicFields"), filters) => {
            collect_pipelines("IMoveObject", Some("dynamicFields"), filters, pipelines);
        }
        ("CoinMetadata", Some("objects"), filters) => {
            collect_pipelines("IObject", Some("objects"), filters, pipelines);
        }
        ("CoinMetadata", Some("receivedTransactions"), filters) => {
            collect_pipelines(
                "IMoveObject",
                Some("receivedTransactions"),
                filters,
                pipelines,
            );
        }
        ("CoinMetadata", Some("supply"), _) => {
            pipelines.insert("consistent".to_string());
        }

        // Epoch fields
        ("Epoch", Some("checkpoints"), filters) => {
            collect_pipelines("Query", Some("checkpoints"), filters, pipelines);
        }

        // Event fields
        ("Event", _, filters) => {
            collect_pipelines("Query", Some("events"), filters, pipelines);
        }

        // IAddressable fields
        ("IAddressable", Some("balance" | "balances" | "multiGetBalances" | "objects"), _) => {
            pipelines.insert("consistent".to_string());
        }

        // IMoveObject fields
        ("IMoveObject", Some("dynamicFields"), _) => {
            pipelines.insert("consistent".to_string());
        }

        // IObject fields
        ("IObject", Some("objects"), _) => {
            pipelines.insert("consistent".to_string());
        }
        ("IObject", Some("receivedTransactions"), mut filters) => {
            filters.insert("affectedAddress".to_string());
            collect_pipelines("Query", Some("transactions"), filters, pipelines);
        }

        // MovePackage fields
        ("MovePackage", field @ Some("balance" | "balances" | "multiGetBalances"), filters) => {
            collect_pipelines("IAddressable", field, filters, pipelines);
        }
        ("MovePackage", Some("objects"), filters) => {
            collect_pipelines("IObject", Some("objects"), filters, pipelines);
        }
        ("MovePackage", Some("receivedTransactions"), filters) => {
            collect_pipelines("IObject", Some("receivedTransactions"), filters, pipelines);
        }

        // Query fields
        ("Query", Some("checkpoints"), _) => {
            pipelines.insert("cp_sequence_numbers".to_string());
        }
        ("Query", Some("coinMetadata"), _) => {
            pipelines.insert("consistent".to_string());
        }
        ("Query", Some("events"), filters) => {
            pipelines.insert("tx_digests".to_string());
            if filters.contains("module") {
                pipelines.insert("ev_emit_mod".to_string());
            } else if filters.contains("type") {
                pipelines.insert("ev_struct_inst".to_string());
            } else if filters.contains("sender") {
                pipelines.insert("ev_struct_inst".to_string());
            } else {
                pipelines.insert("ev_struct_inst".to_string());
            }
        }
        ("Query", Some("objects"), _) => {
            pipelines.insert("consistent".to_string());
        }
        ("Query", Some("transactions"), filters) => {
            pipelines.insert("tx_digests".to_string());
            if filters.contains("sender") {
                pipelines.insert("tx_affected_addresses".to_string());
            }
            if filters.contains("atCheckpoint")
                || filters.contains("afterCheckpoint")
                || filters.contains("beforeCheckpoint")
            {
                pipelines.insert("cp_sequence_numbers".to_string());
            }

            if filters.contains("function") {
                pipelines.insert("tx_calls".to_string());
            } else if filters.contains("affectedAddress") {
                pipelines.insert("tx_affected_addresses".to_string());
            } else if filters.contains("kind") {
                if filters.contains("sentAddress") {
                    pipelines.insert("tx_affected_addresses".to_string());
                } else {
                    pipelines.insert("tx_kinds".to_string());
                }
            } else if filters.contains("affectedObjects") {
                pipelines.insert("tx_affected_objects".to_string());
            }
        }
        ("TransactionEffects", Some("balanceChanges"), _) => {
            pipelines.insert("tx_balance_changes".to_string());
            pipelines.insert("tx_digests".to_string());
        }
        (_, _, _) => (),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::BTreeSet;

    fn test_collect_pipelines(
        type_: &str,
        field: Option<&str>,
        filters: BTreeSet<String>,
    ) -> BTreeSet<String> {
        let mut pipelines = BTreeSet::new();
        collect_pipelines(type_, field, filters, &mut pipelines);
        pipelines
    }

    #[test]
    fn test_address_transactions() {
        let result = test_collect_pipelines("Address", Some("transactions"), BTreeSet::new());
        assert!(result.contains("tx_digests"));
        assert!(result.contains("tx_affected_addresses"));
    }

    #[test]
    fn test_address_consistent_fields() {
        let result = test_collect_pipelines("Address", Some("balance"), BTreeSet::new());
        assert!(result.contains("consistent"));
    }

    #[test]
    fn test_address_other_fields() {
        let result = test_collect_pipelines("Address", Some("defaultSuinsName"), BTreeSet::new());
        assert!(result.is_empty());
    }

    #[test]
    fn test_checkpoint_transactions() {
        let result = test_collect_pipelines("Checkpoint", Some("transactions"), BTreeSet::new());
        assert!(result.contains("cp_sequence_numbers"));
        assert!(result.contains("tx_digests"));
    }

    #[test]
    fn test_coin_metadata() {
        let result = test_collect_pipelines("CoinMetadata", Some("balance"), BTreeSet::new());
        assert!(result.contains("consistent"));
    }

    #[test]
    fn test_epoch_checkpoints() {
        let result = test_collect_pipelines("Epoch", Some("checkpoints"), BTreeSet::new());
        assert!(result.contains("cp_sequence_numbers"));
    }

    #[test]
    fn test_event() {
        let result = test_collect_pipelines("Event", None, BTreeSet::new());
        assert!(result.contains("ev_struct_inst"));
        assert!(result.contains("tx_digests"));
    }

    #[test]
    fn test_iobject_received_transactions() {
        let result =
            test_collect_pipelines("IObject", Some("receivedTransactions"), BTreeSet::new());
        assert!(result.contains("tx_digests"));
        assert!(result.contains("tx_affected_addresses"));
    }

    #[test]
    fn test_move_package() {
        let result = test_collect_pipelines("MovePackage", Some("balance"), BTreeSet::new());
        assert!(result.contains("consistent"));
    }

    #[test]
    fn test_query_address() {
        let result = test_collect_pipelines("Query", Some("address"), BTreeSet::new());
        assert!(result.is_empty());
    }

    #[test]
    fn test_query_checkpoints() {
        let result = test_collect_pipelines("Query", Some("checkpoints"), BTreeSet::new());
        assert!(result.contains("cp_sequence_numbers"));
    }

    #[test]
    fn test_query_coin_metadata() {
        let result = test_collect_pipelines("Query", Some("coinMetadata"), BTreeSet::new());
        assert!(result.contains("consistent"));
    }

    #[test]
    fn test_query_events_no_filters() {
        let result = test_collect_pipelines("Query", Some("events"), BTreeSet::new());
        assert!(result.contains("tx_digests"));
        assert!(result.contains("ev_struct_inst"));
    }

    #[test]
    fn test_query_events_with_module_and_sender_filter() {
        let result = test_collect_pipelines(
            "Query",
            Some("events"),
            BTreeSet::from_iter(vec!["module".to_string(), "sender".to_string()]),
        );
        assert!(result.contains("tx_digests"));
        assert!(result.contains("ev_emit_mod"));
    }

    #[test]
    fn test_query_events_with_sender_filter() {
        let result = test_collect_pipelines(
            "Query",
            Some("events"),
            BTreeSet::from_iter(vec!["sender".to_string()]),
        );
        assert!(result.contains("tx_digests"));
        assert!(result.contains("ev_struct_inst"));
    }

    #[test]
    fn test_query_objects() {
        let result = test_collect_pipelines("Query", Some("objects"), BTreeSet::new());
        assert!(result.contains("consistent"));
    }

    #[test]
    fn test_query_transactions_no_filters() {
        let result = test_collect_pipelines("Query", Some("transactions"), BTreeSet::new());
        assert!(result.contains("tx_digests"));
    }

    #[test]
    fn test_query_transactions_kind_filter() {
        let result = test_collect_pipelines(
            "Query",
            Some("transactions"),
            BTreeSet::from_iter(vec!["kind".to_string()]),
        );
        assert!(result.contains("tx_digests"));
        assert!(result.contains("tx_kinds"));
    }

    #[test]
    fn test_query_transactions_multiple_filters() {
        let result = test_collect_pipelines(
            "Query",
            Some("transactions"),
            BTreeSet::from_iter(vec![
                "kind".to_string(),
                "sentAddress".to_string(),
                "atCheckpoint".to_string(),
            ]),
        );
        assert!(result.contains("tx_digests"));
        assert!(result.contains("tx_affected_addresses"));
        assert!(result.contains("cp_sequence_numbers"));
    }

    #[test]
    fn test_transaction_effects_balance_changes() {
        let result = test_collect_pipelines(
            "TransactionEffects",
            Some("balanceChanges"),
            BTreeSet::new(),
        );
        assert!(result.contains("tx_balance_changes"));
        assert!(result.contains("tx_digests"));
    }

    #[test]
    fn test_catch_all() {
        let invalid: BTreeSet<String> =
            test_collect_pipelines("UnknownType", Some("field"), BTreeSet::new());
        assert!(invalid.is_empty());
        let valid: BTreeSet<String> =
            test_collect_pipelines("Address", Some("digests"), BTreeSet::new());
        assert!(valid.is_empty());
    }
}
