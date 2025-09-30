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
pub(crate) struct RetentionKey {
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
        retention_key: RetentionKey,
    ) -> Result<Self, RpcError> {
        let watermarks: &Arc<Watermarks> = ctx.data()?;
        let mut ps = BTreeSet::new();
        pipelines(
            &retention_key.type_,
            retention_key.field.as_deref(),
            retention_key.filters.unwrap_or_default(),
            &mut ps,
        );

        let first = ps
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
fn pipelines(type_: &str, field: Option<&str>, filters: Vec<String>, ps: &mut BTreeSet<String>) {
    match (type_, field, filters) {
        // Address fields
        ("Address", Some("transactions"), mut filters) => {
            filters.push("affectedAddress".to_string());
            pipelines("Query", Some("transactions"), filters, ps);
        }
        ("Address", Some("dynamicFields"), _) => {
            pipelines("IMoveObject", Some("dynamicFields"), vec![], ps);
        }
        ("Address", field, filters) => {
            pipelines("IAddressable", field, filters, ps);
        }

        // Checkpoint fields
        ("Checkpoint", Some("transactions"), mut filters) => {
            filters.push("atCheckpoint".to_string());
            pipelines("Query", Some("transactions"), filters, ps);
        }

        // CoinMetadata fields
        ("CoinMetadata", Some("supply"), _) => {
            pipelines("Query", Some("coinMetadata"), vec![], ps);
        }
        ("CoinMetadata", field, filters) => {
            pipelines("IMoveObject", field, vec![], ps);
            pipelines("IAddressable", field, vec![], ps);
            pipelines("IObject", field, filters, ps);
        }

        // Epoch fields
        ("Epoch", Some("checkpoints"), filters) => {
            pipelines("Query", Some("checkpoints"), filters, ps);
        }

        // Event fields
        ("Event", _, _) => {
            pipelines("Query", Some("events"), vec![], ps);
        }

        // IAddressable fields
        ("IAddressable", Some("balance"), _)
        | ("IAddressable", Some("balances"), _)
        | ("IAddressable", Some("multiGetBalances"), _)
        | ("IAddressable", Some("objects"), _) => {
            ps.insert("consistent".to_string());
        }

        // IMoveObject fields
        ("IMoveObject", Some("dynamicFields"), _) => {
            ps.insert("consistent".to_string());
        }

        // IObject fields
        ("IObject", Some("objects"), _) => {
            ps.insert("consistent".to_string());
        }
        ("IObject", Some("receivedTransactions"), mut filters) => {
            filters.push("affectedAddress".to_string());
            pipelines("Query", Some("transactions"), filters, ps);
        }

        // Package fields
        ("Package", field, filters) => {
            pipelines("IAddressable", field, vec![], ps);
            pipelines("IObject", field, filters, ps);
        }

        // Query fields
        ("Query", Some("checkpoints"), _) => {
            ps.insert("cp_sequence_numbers".to_string());
        }
        ("Query", Some("coinMetadata"), _) => {
            ps.insert("consistent".to_string());
        }
        ("Query", Some("events"), filters) => {
            ps.insert("tx_digests".to_string());
            if filters.is_empty() {
                ps.insert("ev_struct_inst".to_string());
            } else {
                for filter in filters {
                    if filter == "sender" {
                        ps.insert("ev_emit_mod".to_string());
                        ps.insert("ev_struct_inst".to_string());
                    } else if filter == "module" {
                        ps.insert("ev_emit_mod".to_string());
                    } else if filter == "type" {
                        ps.insert("ev_struct_inst".to_string());
                    }
                }
            }
        }
        ("Query", Some("objects"), _) => {
            ps.insert("consistent".to_string());
        }
        ("Query", Some("transactions"), filters) => {
            ps.insert("tx_digests".to_string());
            for filter in filters {
                if filter == "affectedAddress" || filter == "sentAddress" || filter == "kind" {
                    ps.insert("tx_affected_addresses".to_string());
                }
                if filter == "kind" {
                    ps.insert("tx_kinds".to_string());
                }
                if filter == "function" {
                    ps.insert("tx_calls".to_string());
                }
                if filter == "affectedObjects" {
                    ps.insert("tx_affected_objects".to_string());
                }
                if filter == "atCheckpoint" {
                    ps.insert("cp_sequence_numbers".to_string());
                }
            }
        }
        ("TransactionEffects", Some("balanceChanges"), _) => {
            ps.insert("tx_balance_changes".to_string());
            ps.insert("tx_digests".to_string());
        }
        (_, _, _) => (),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::BTreeSet;

    fn test_pipelines(type_: &str, field: Option<&str>, filters: Vec<String>) -> BTreeSet<String> {
        let mut ps = BTreeSet::new();
        pipelines(type_, field, filters, &mut ps);
        ps
    }

    #[test]
    fn test_address_transactions() {
        let result = test_pipelines("Address", Some("transactions"), vec![]);
        assert!(result.contains("tx_digests"));
        assert!(result.contains("tx_affected_addresses"));
    }

    #[test]
    fn test_address_consistent_fields() {
        let result = test_pipelines("Address", Some("balance"), vec![]);
        assert!(result.contains("consistent"));
    }

    #[test]
    fn test_address_other_fields() {
        let result = test_pipelines("Address", Some("defaultSuinsName"), vec![]);
        assert!(result.is_empty());
    }

    #[test]
    fn test_checkpoint_transactions() {
        let result = test_pipelines("Checkpoint", Some("transactions"), vec![]);
        assert!(result.contains("cp_sequence_numbers"));
        assert!(result.contains("tx_digests"));
    }

    #[test]
    fn test_coin_metadata() {
        let result = test_pipelines("CoinMetadata", Some("balance"), vec![]);
        assert!(result.contains("consistent"));
    }

    #[test]
    fn test_epoch_checkpoints() {
        let result = test_pipelines("Epoch", Some("checkpoints"), vec![]);
        assert!(result.contains("cp_sequence_numbers"));
    }

    #[test]
    fn test_event() {
        let result = test_pipelines("Event", None, vec![]);
        assert!(result.contains("ev_struct_inst"));
        assert!(result.contains("tx_digests"));
    }

    #[test]
    fn test_iobject_received_transactions() {
        let result = test_pipelines("IObject", Some("receivedTransactions"), vec![]);
        assert!(result.contains("tx_digests"));
        assert!(result.contains("tx_affected_addresses"));
    }

    #[test]
    fn test_package() {
        let result = test_pipelines("Package", Some("balance"), vec![]);
        assert!(result.contains("consistent"));
    }

    #[test]
    fn test_query_address() {
        let result = test_pipelines("Query", Some("address"), vec![]);
        assert!(result.is_empty());
    }

    #[test]
    fn test_query_checkpoints() {
        let result = test_pipelines("Query", Some("checkpoints"), vec![]);
        assert!(result.contains("cp_sequence_numbers"));
    }

    #[test]
    fn test_query_coin_metadata() {
        let result = test_pipelines("Query", Some("coinMetadata"), vec![]);
        assert!(result.contains("consistent"));
    }

    #[test]
    fn test_query_events_no_filters() {
        let result = test_pipelines("Query", Some("events"), vec![]);
        assert!(result.contains("tx_digests"));
        assert!(result.contains("ev_struct_inst"));
    }

    #[test]
    fn test_query_events_with_module_filter() {
        let result = test_pipelines("Query", Some("events"), vec!["module".to_string()]);
        assert!(result.contains("tx_digests"));
        assert!(result.contains("ev_emit_mod"));
    }

    #[test]
    fn test_query_events_with_sender_filter() {
        let result = test_pipelines("Query", Some("events"), vec!["sender".to_string()]);
        assert!(result.contains("tx_digests"));
        assert!(result.contains("ev_emit_mod"));
        assert!(result.contains("ev_struct_inst"));
    }

    #[test]
    fn test_query_objects() {
        let result = test_pipelines("Query", Some("objects"), vec![]);
        assert!(result.contains("consistent"));
    }

    #[test]
    fn test_query_transactions_no_filters() {
        let result = test_pipelines("Query", Some("transactions"), vec![]);
        assert!(result.contains("tx_digests"));
    }

    #[test]
    fn test_query_transactions_affected_objects_filter() {
        let result = test_pipelines(
            "Query",
            Some("transactions"),
            vec!["affectedObjects".to_string()],
        );
        assert!(result.contains("tx_digests"));
        assert!(result.contains("tx_affected_objects"));
    }

    #[test]
    fn test_query_transactions_multiple_filters() {
        let result = test_pipelines(
            "Query",
            Some("transactions"),
            vec![
                "affectedAddress".to_string(),
                "kind".to_string(),
                "function".to_string(),
                "atCheckpoint".to_string(),
            ],
        );
        assert!(result.contains("tx_digests"));
        assert!(result.contains("tx_affected_addresses"));
        assert!(result.contains("tx_kinds"));
        assert!(result.contains("tx_calls"));
        assert!(result.contains("cp_sequence_numbers"));
    }

    #[test]
    fn test_transaction_effects_balance_changes() {
        let result = test_pipelines("TransactionEffects", Some("balanceChanges"), vec![]);
        assert!(result.contains("tx_balance_changes"));
        assert!(result.contains("tx_digests"));
    }

    #[test]
    fn test_catch_all() {
        let invalid: BTreeSet<String> = test_pipelines("UnknownType", Some("field"), vec![]);
        assert!(invalid.is_empty());
        let valid: BTreeSet<String> = test_pipelines("Address", Some("digests"), vec![]);
        assert!(valid.is_empty());
    }
}
