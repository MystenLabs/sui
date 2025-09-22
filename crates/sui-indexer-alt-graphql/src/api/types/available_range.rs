// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use async_graphql::{Context, InputObject, Object};
use std::sync::Arc;

use crate::{error::RpcError, scope::Scope, task::watermark::Watermarks};

use super::checkpoint::Checkpoint;

/// Identifies a GraphQL query component that is used to determine the range of checkpoints for which data is available (for data that can be tied to a particular checkpoint)
///
/// Both `type_` and `field` are required. The `filter` is optional and provides retention information for filtered queries.
#[derive(InputObject, Debug, Clone, Eq, PartialEq)]
pub(crate) struct RetentionKey {
    /// The GraphQL type to check retention for
    pub(crate) type_: String,

    /// The specific field within the type to check retention for
    pub(crate) field: String,

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
        let pipelines = pipelines(
            &retention_key.type_,
            &retention_key.field,
            retention_key.filters,
        );

        let lo_checkpoint =
            pipelines
                .iter()
                .try_fold(0, |acc: u64, pipeline| -> Result<u64, RpcError> {
                    let watermark = watermarks.pipeline_lo_watermark(pipeline)?;
                    let checkpoint = watermark.checkpoint();
                    Ok(acc.max(checkpoint))
                })?;

        Ok(Self {
            scope: scope.clone(),
            first: lo_checkpoint,
        })
    }
}

/// Maps GraphQL query components to watermark pipeline names.
///
/// Determines which watermark pipelines are relevant for a given GraphQL query.
/// The pipeline names are used to query watermark data to determine the
/// checkpoint sequence range (available range) for which data is available.
///
fn pipelines(type_: &str, field: &str, filters: Option<Vec<String>>) -> Vec<&'static str> {
    let mut filters = filters.unwrap_or_default();
    match (type_, field) {
        // Address queries
        ("Address", "asObject") => vec!["obj_versions"],
        ("Address", "balance") => pipelines("IAddressable", "balance", None),
        ("Address", "balances") => pipelines("IAddressable", "balances", None),
        ("Address", "defaultSuiNsName") => pipelines("IAddressable", "defaultSuiNsName", None),
        ("Address", "dynamicField") => vec!["obj_versions"],
        ("Address", "dynamicFields") => vec!["consistent"],
        ("Address", "dynamicObjectField") => vec!["obj_versions"],
        ("Address", "multiGetDynamicFields") => vec!["obj_versions"],
        ("Address", "multiGetDynamicObjectFields") => vec!["obj_versions"],
        ("Address", "multiGetBalances") => pipelines("IAddressable", "multiGetBalances", None),
        ("Address", "objects") => pipelines("IAddressable", "objects", Some(filters)),
        ("Address", "transactions") => {
            filters.push("affectedAddress".to_string());
            pipelines("Query", "transactions", Some(filters))
        }

        // Checkpoint queries
        ("Checkpoint", "artifactsDigest") => vec!["cp_sequence_numbers"],
        ("Checkpoint", "digest") => vec!["cp_sequence_numbers"],
        ("Checkpoint", "contentDigest") => vec!["cp_sequence_numbers"],
        ("Checkpoint", "epoch") => vec!["cp_sequence_numbers"],
        ("Checkpoint", "networkTotalTransactions") => vec!["cp_sequence_numbers"],
        ("Checkpoint", "previousCheckpointDigest") => vec!["cp_sequence_numbers"],
        ("Checkpoint", "query") => vec!["cp_sequence_numbers"],
        ("Checkpoint", "rollingGasSummary") => vec!["cp_sequence_numbers"],
        ("Checkpoint", "sequenceNumber") => vec!["cp_sequence_numbers"],
        ("Checkpoint", "summaryBcs") => vec!["cp_sequence_numbers"],
        ("Checkpoint", "timestamp") => vec!["cp_sequence_numbers"],
        ("Checkpoint", "validatorSignatures") => vec!["cp_sequence_numbers, kv_epoch_starts"],
        ("Checkpoint", "transactions") => pipelines("Query", "transactions", Some(filters)),

        // CoinMetadata queries
        ("CoinMetadata", "address") => pipelines("IAddressable", "address", None),
        ("CoinMetadata", "balance") => pipelines("IAddressable", "balance", None),
        ("CoinMetadata", "balances") => pipelines("IAddressable", "balances", None),
        ("CoinMetadata", "contents") => pipelines("IMoveObject", "contents", None),
        ("CoinMetadata", "decimals") => vec!["consistent"],
        ("CoinMetadata", "defaultSuiNsName") => pipelines("IAddressable", "defaultSuiNsName", None),
        ("CoinMetadata", "description") => vec!["consistent"],
        ("CoinMetadata", "dynamicField") => pipelines("IMoveObject", "dynamicField", None),
        ("CoinMetadata", "dynamicFields") => pipelines("IMoveObject", "dynamicFields", None),
        ("CoinMetadata", "dynamicObjectField") => {
            pipelines("IMoveObject", "dynamicObjectField", None)
        }
        ("CoinMetadata", "hasPublicTransfer") => {
            pipelines("IMoveObject", "hasPublicTransfer", None)
        }
        ("CoinMetadata", "multiGetDynamicFields") => {
            pipelines("IMoveObject", "multiGetDynamicFields", None)
        }
        ("CoinMetadata", "multiGetBalances") => pipelines("IAddressable", "multiGetBalances", None),
        ("CoinMetadata", "multiGetDynamicObjectFields") => {
            pipelines("IMoveObject", "multiGetDynamicObjectFields", None)
        }
        ("CoinMetadata", "moveObjectBcs") => pipelines("IMoveObject", "moveObjectBcs", None),
        ("CoinMetadata", "objectAt") => pipelines("IObject", "objectAt", None),
        ("CoinMetadata", "objectVersionsAfter") => {
            pipelines("IObject", "objectVersionsAfter", None)
        }
        ("CoinMetadata", "objectVersionsBefore") => {
            pipelines("IObject", "objectVersionsBefore", None)
        }
        ("CoinMetadata", "objects") => pipelines("IObject", "objects", None),
        ("CoinMetadata", "transactions") => pipelines("Query", "transactions", Some(filters)),

        // Epoch queries
        ("Epoch", "epochId") => vec!["kv_epoch_starts"],
        ("Epoch", "checkpoints") => pipelines("Query", "checkpoints", Some(filters)),
        ("Epoch", "coinDenyList") => {
            vec!["kv_epoch_starts", "obj_versions"]
        }
        ("Epoch", "endTimestamp") => vec!["kv_epoch_ends"],
        ("Epoch", "fundInflow") => vec!["kv_epoch_ends"],
        ("Epoch", "fundOutflow") => vec!["kv_epoch_ends"],
        ("Epoch", "fundSize") => vec!["kv_epoch_ends"],
        ("Epoch", "liveObjectSetDigest") => vec!["kv_epoch_ends"],
        ("Epoch", "netInflow") => vec!["kv_epoch_ends"],
        ("Epoch", "protocolConfigs") => vec!["kv_epoch_starts"],
        ("Epoch", "referenceGasPrice") => vec!["kv_epoch_starts"],
        ("Epoch", "safeMode") => vec!["kv_epoch_starts"],
        ("Epoch", "startTimestamp") => vec!["kv_epoch_starts"],
        ("Epoch", "storageFund") => vec!["kv_epoch_ends"],
        ("Epoch", "systemPackages") => vec!["kv_epoch_starts", "kv_packages"],
        ("Epoch", "systemParameters") => vec!["kv_epoch_starts"],
        ("Epoch", "systemStakeSubsidy") => vec!["kv_epoch_starts"],
        ("Epoch", "systemStateVersion") => vec!["kv_epoch_starts"],
        ("Epoch", "totalCheckpoints") => vec!["kv_epoch_starts"],
        ("Epoch", "totalGasFees") => vec!["kv_epoch_ends"],
        ("Epoch", "totalStakeRewards") => vec!["kv_epoch_ends"],
        ("Epoch", "totalStakeSubsidies") => vec!["kv_epoch_ends"],
        ("Epoch", "totalTransactions") => vec!["cp_sequence_numbers", "kv_epoch_ends"],
        ("Epoch", "transactions") => pipelines("Query", "transactions", None),
        ("Epoch", "validatorSet") => vec!["kv_epoch_starts"],

        // Event queries
        ("Event", "contents") => vec!["ev_struct_inst", "tx_digests"],
        ("Event", "eventBcs") => vec!["ev_struct_inst", "tx_digests"],
        ("Event", "sender") => vec!["ev_struct_inst", "ev_emit_mod", "tx_digests"],
        ("Event", "sequenceNumber") => vec!["ev_struct_inst", "tx_digests"],
        ("Event", "timestamp") => vec!["ev_struct_inst", "tx_digests"],
        ("Event", "transaction") => vec!["ev_struct_inst", "ev_emit_mod", "tx_digests"],
        ("Event", "transactionModule") => pipelines("IMoveObject", "contents", None),

        // IAddressable queries
        ("IAddressable", "address") => vec!["obj_versions"],
        ("IAddressable", "balance") => vec!["consistent"],
        ("IAddressable", "balances") => vec!["consistent"],
        ("IAddressable", "defaultSuiNsName") => vec!["obj_versions"],
        ("IAddressable", "multiGetBalances") => vec!["consistent"],
        ("IAddressable", "objects") => vec!["consistent"],

        // IObject queries
        ("IObject", "contents") => vec!["obj_versions"],
        ("IObject", "digest") => vec!["obj_versions"],
        ("IObject", "objectAt") => vec!["obj_versions"],
        ("IObject", "objectBcs") => vec!["obj_versions"],
        ("IObject", "objectVersionsAfter") => vec!["obj_versions"],
        ("IObject", "objectVersionsBefore") => vec!["obj_versions"],
        ("IObject", "objects") => vec!["consistent"],
        ("IObject", "owner") => vec!["obj_versions"],
        ("IObject", "previousTransaction") => vec!["obj_versions", "tx_digests"],
        ("IObject", "receivedTransactions") => {
            filters.push("affectedAddress".to_string());
            pipelines("Query", "transactions", Some(filters))
        }
        ("IObject", "storageRebate") => vec!["obj_versions"],
        ("IObject", "version") => vec!["obj_versions"],

        // IMoveObject queries
        ("IMoveObject", "contents") => vec!["obj_versions"],
        ("IMoveObject", "dynamicField") => vec!["obj_versions"],
        ("IMoveObject", "dynamicFields") => vec!["consistent"],
        ("IMoveObject", "dynamicObjectField") => vec!["obj_versions"],
        ("IMoveObject", "hasPublicTransfer") => vec!["obj_versions"],
        ("IMoveObject", "moveObjectBcs") => vec!["obj_versions"],
        ("IMoveObject", "multiGetDynamicFields") => vec!["obj_versions"],
        ("IMoveObject", "multiGetDynamicObjectFields") => vec!["obj_versions"],

        // Object queries
        ("Object", "address") => pipelines("IAddressable", "address", None),
        ("Object", "asMoveObject") => pipelines("IMoveObject", "contents", None),
        ("Object", "asMovePackage") => pipelines("IMoveObject", "contents", None),
        ("Object", "balance") => pipelines("IAddressable", "balance", None),
        ("Object", "balances") => pipelines("IAddressable", "balances", None),
        ("Object", "defaultSuiNsName") => pipelines("IAddressable", "defaultSuiNsName", None),
        ("Object", "digest") => vec!["obj_versions"],
        ("Object", "dynamicField") => pipelines("IMoveObject", "dynamicField", None),
        ("Object", "dynamicFields") => pipelines("IMoveObject", "dynamicFields", None),
        ("Object", "dynamicObjectField") => pipelines("IMoveObject", "dynamicObjectField", None),
        ("Object", "multiGetBalances") => pipelines("IAddressable", "multiGetBalances", None),
        ("Object", "multiGetDynamicFields") => {
            pipelines("IMoveObject", "multiGetDynamicFields", None)
        }
        ("Object", "multiGetDynamicObjectFields") => {
            pipelines("IMoveObject", "multiGetDynamicObjectFields", None)
        }
        ("Object", "objectAt") => pipelines("IObject", "objectAt", None),
        ("Object", "objectBcs") => pipelines("IObject", "objectBcs", None),
        ("Object", "objectVersionsAfter") => pipelines("IObject", "objectVersionsAfter", None),
        ("Object", "objectVersionsBefore") => pipelines("IObject", "objectVersionsBefore", None),
        ("Object", "objects") => pipelines("IObject", "objects", None),
        ("Object", "owner") => pipelines("IObject", "owner", None),
        ("Object", "previousTransaction") => pipelines("IObject", "previousTransaction", None),
        ("Object", "receivedTransactions") => pipelines("IObject", "receivedTransactions", None),
        ("Object", "storageRebate") => pipelines("IObject", "storageRebate", None),
        ("Object", "version") => pipelines("IObject", "version", None),

        // Package queries
        ("Package", "address") => pipelines("IAddressable", "address", None),
        ("Package", "balance") => pipelines("IAddressable", "balance", None),
        ("Package", "balances") => pipelines("IAddressable", "balances", None),
        ("Package", "defaultSuiNsName") => pipelines("IAddressable", "defaultSuiNsName", None),
        ("Package", "digest") => pipelines("IObject", "digest", None),
        ("Package", "linkage") => pipelines("IObject", "contents", None),
        ("Package", "module") => pipelines("IObject", "contents", None),
        ("Package", "moduleBcs") => pipelines("IObject", "contents", None),
        ("Package", "modules") => pipelines("IObject", "contents", None),
        ("Package", "multiGetBalances") => pipelines("IAddressable", "multiGetBalances", None),
        ("Package", "objectAt") => pipelines("IObject", "objectAt", None),
        ("Package", "objectBcs") => pipelines("IObject", "objectBcs", None),
        ("Package", "objectVersionsAfter") => pipelines("IObject", "objectVersionsAfter", None),
        ("Package", "objectVersionsBefore") => pipelines("IObject", "objectVersionsBefore", None),
        ("Package", "objects") => pipelines("IObject", "objects", None),
        ("Package", "owner") => pipelines("IObject", "owner", None),
        ("Package", "packageAt") => pipelines("IAddressable", "address", None),
        ("Package", "packageBcs") => pipelines("IObject", "contents", None),
        ("Package", "packageVersionsAfter") => pipelines("IObject", "version", None),
        ("Package", "packageVersionsBefore") => pipelines("IObject", "version", None),
        ("Package", "previousTransaction") => pipelines("IObject", "previousTransaction", None),
        ("Package", "receivedTransactions") => pipelines("IObject", "receivedTransactions", None),
        ("Package", "storageRebate") => pipelines("IObject", "storageRebate", None),
        ("Package", "typeOrigins") => pipelines("IObject", "contents", None),
        ("Package", "version") => pipelines("IObject", "version", None),

        // Protocol config queries
        ("ProtocolConfigs", "protocolVersion") => vec!["kv_epoch_starts"],
        ("ProtocolConfigs", "featureFlags") => vec!["kv_epoch_starts"],
        ("ProtocolConfigs", "configs") => vec!["kv_epoch_starts"],

        // Query
        ("Query", "address") => pipelines("IAddressable", "address", None),
        ("Query", "checkpoint") => vec!["cp_sequence_numbers"],
        ("Query", "checkpoints") => vec!["cp_sequence_numbers"],
        ("Query", "coinMetadata") => {
            let mut pipelines = vec!["consistent"];
            for filter in filters {
                if filter == "version" {
                    pipelines.push("obj_versions");
                }
            }
            pipelines
        }
        ("Query", "epoch") => vec!["kv_epoch_starts"],
        ("Query", "epochs") => vec!["kv_epoch_starts"],
        ("Query", "event") => vec!["ev_struct_inst", "ev_emit_mod"],
        ("Query", "events") => {
            let mut pipelines = vec!["ev_struct_inst"];
            for filter in filters {
                if filter == "module" || filter == "sender" {
                    pipelines.push("ev_emit_mod");
                }
            }
            pipelines
        }
        ("Query", "multiGetCheckpoints") => pipelines("Query", "checkpoint", None),
        ("Query", "multiGetEpochs") => pipelines("Query", "epoch", None),
        ("Query", "multiGetObjects") => pipelines("Query", "object", None),
        ("Query", "multiGetPackages") => pipelines("Query", "package", None),
        ("Query", "multiGetTransactionEffects") => pipelines("Query", "transactionEffects", None),
        ("Query", "multiGetTransactions") => pipelines("Query", "transaction", None),
        ("Query", "multiGetTypes") => pipelines("Query", "type", None),
        ("Query", "object") => vec!["obj_versions"],
        ("Query", "objects") => vec!["consistent"],
        ("Query", "objectVersions") => vec!["obj_versions"],
        ("Query", "package") => vec!["kv_packages"],
        ("Query", "packages") => vec!["cp_sequence_numbers", "kv_packages"],
        ("Query", "packageVersions") => vec!["kv_packages"],
        ("Query", "protocolConfigs") => vec!["kv_epoch_starts"],
        ("Query", "simulateTransaction") => vec![],
        ("Query", "suinsName") => vec!["obj_versions"],
        ("Query", "transaction") => vec!["tx_digests"],
        ("Query", "transactionEffects") => vec!["cp_sequence_numbers", "kv_transactions"],
        ("Query", "transactions") => {
            let mut pipelines = vec!["tx_digests"];
            for filter in filters {
                if filter == "affectedAddress" || filter == "sentAddress" || filter == "kind" {
                    pipelines.push("tx_affected_addresses");
                }
                if filter == "kind" {
                    pipelines.push("tx_kinds")
                }
                if filter == "function" {
                    pipelines.push("tx_calls")
                }
                if filter == "affectedObjects" {
                    pipelines.push("tx_affected_objects")
                }
            }
            pipelines
        }
        ("Query", "type") => vec!["kv_packages"],

        // Transaction queries
        ("Transaction", "digest") => vec!["kv_transactions"],
        ("Transaction", "effects") => vec!["cp_sequence_numbers", "kv_transactions"],
        ("Transaction", "expiration") => vec!["kv_transactions"],
        ("Transaction", "gasInput") => vec!["kv_transactions"],
        ("Transaction", "kind") => vec!["kv_transactions"],
        ("Transaction", "sender") => vec!["kv_transactions"],
        ("Transaction", "signatures") => vec!["kv_transactions"],
        ("Transaction", "transactionBcs") => vec!["kv_transactions"],

        // TransactionEffects queries
        ("TransactionEffects", "balanceChanges") => vec![
            "cp_sequence_numbers",
            "kv_transactions",
            "tx_balance_changes",
            "tx_digests",
        ],
        ("TransactionEffects", "checkpoint") => vec!["cp_sequence_numbers", "kv_transactions"],
        ("TransactionEffects", "dependencies") => vec!["cp_sequence_numbers", "kv_transactions"],
        ("TransactionEffects", "digest") => vec!["cp_sequence_numbers", "kv_transactions"],
        ("TransactionEffects", "effectsBcs") => vec!["cp_sequence_numbers", "kv_transactions"],
        ("TransactionEffects", "effectsDigest") => vec!["cp_sequence_numbers", "kv_transactions"],
        ("TransactionEffects", "epoch") => {
            vec!["cp_sequence_numbers", "kv_transactions", "kv_epoch_starts"]
        }
        ("TransactionEffects", "events") => vec!["cp_sequence_numbers", "kv_transactions"],
        ("TransactionEffects", "executionError") => {
            vec!["cp_sequence_numbers", "kv_transactions", "kv_packages"]
        }
        ("TransactionEffects", "gasEffects") => vec!["cp_sequence_numbers", "kv_transactions"],
        ("TransactionEffects", "lamportVersion") => vec!["cp_sequence_numbers", "kv_transactions"],
        ("TransactionEffects", "objectChanges") => vec!["cp_sequence_numbers", "kv_transactions"],
        ("TransactionEffects", "status") => vec!["cp_sequence_numbers", "kv_transactions"],
        ("TransactionEffects", "timestamp") => vec!["cp_sequence_numbers", "kv_transactions"],
        ("TransactionEffects", "transaction") => vec!["cp_sequence_numbers", "kv_transactions"],
        ("TransactionEffects", "unchangedConsensusObjects") => {
            vec!["cp_sequence_numbers", "kv_transactions"]
        }
        (_, _) => vec![],
    }
}
