// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use std::collections::HashMap;
use std::sync::Arc;

use anyhow::Context;
use async_graphql::dataloader::DataLoader;
use prometheus::Registry;
use sui_rpc::proto::sui::rpc::v2 as grpc;
use sui_types::base_types::ObjectID;
use sui_types::crypto::AuthorityQuorumSignInfo;
use sui_types::digests::CheckpointDigest;
use sui_types::digests::TransactionDigest;
use sui_types::digests::TransactionEffectsDigest;
use sui_types::effects::TransactionEffects;
use sui_types::effects::TransactionEffectsAPI;
use sui_types::effects::TransactionEvents;
use sui_types::event::Event;
use sui_types::message_envelope::Message;
use sui_types::messages_checkpoint::CheckpointContents;
use sui_types::messages_checkpoint::CheckpointSummary;
use sui_types::object::Object;
use sui_types::signature::GenericSignature;
use sui_types::transaction::TransactionData;
use tonic::transport::Uri;

use crate::alpha_ledger_grpc_reader::AlphaLedgerGrpcReader;
use crate::checkpoints::CheckpointKey;
use crate::error::Error;
use crate::events::TransactionEventsKey;
use crate::ledger_grpc_reader::CheckpointedTransaction;
use crate::ledger_grpc_reader::LedgerGrpcArgs;
use crate::ledger_grpc_reader::LedgerGrpcReader;
use crate::ledger_grpc_reader::MAX_BATCH_GET_OBJECTS;
use crate::ledger_grpc_reader::MAX_BATCH_GET_TRANSACTIONS;
use crate::objects::VersionedObjectKey;
use crate::transactions::ProtoEffectsKey;
use crate::transactions::TransactionKey;
use crate::transactions::TransactionTimestampKey;

/// Arguments for configuring KV store access via the Ledger gRPC service.
#[derive(clap::Args, Debug, Clone, Default)]
pub struct KvArgs {
    /// Maximum gRPC decoding message size for KV responses, in bytes.
    #[arg(long)]
    pub kv_max_decoding_message_size: Option<usize>,

    /// gRPC endpoint URL for the ledger service (e.g., archive.mainnet.sui.io)
    #[arg(long)]
    pub ledger_grpc_url: Option<Uri>,

    /// Whether the configured ledger gRPC service serves the List APIs (e.g. bitmap-backed
    /// transaction pagination). When unset, treated as `false`.
    #[arg(long, alias = "experimental-query-apis")]
    pub enable_list_apis: Option<bool>,

    /// Time spent waiting for a request to the kv store to complete, in milliseconds.
    #[arg(long)]
    pub kv_statement_timeout_ms: Option<u64>,
}

/// A loader for point lookups against the Ledger gRPC service.
/// Supported lookups:
/// - Objects by id and version
/// - Checkpoints by sequence number
/// - Transactions by digest
#[derive(Clone)]
pub struct KvLoader(Arc<DataLoader<LedgerGrpcReader>>);

/// A wrapper for the contents of a transaction, either from Ledger gRPC or just executed.
#[allow(clippy::large_enum_variant)]
#[derive(Clone)]
pub enum TransactionContents {
    LedgerGrpc(CheckpointedTransaction),
    ExecutedTransaction(ExecutedTransactionData),
}

/// Transaction data from a gRPC execution or streaming response.
#[derive(Clone)]
pub struct ExecutedTransactionData {
    pub effects: Box<TransactionEffects>,
    pub events: Vec<Arc<Event>>,
    pub transaction_data: Box<TransactionData>,
    pub signatures: Vec<GenericSignature>,
    pub balance_changes: Vec<grpc::BalanceChange>,
    /// The proto TransactionEffects from gRPC, if available.
    /// Contains fully-rendered effects with object types and clever errors.
    pub proto_effects: Option<grpc::TransactionEffects>,
    /// The proto Transaction from gRPC, if available.
    /// Contains the fully-rendered transaction.
    pub proto_transaction: Option<grpc::Transaction>,
    /// Checkpoint timestamp. Set for streamed/checkpointed transactions, None for mutations.
    pub timestamp_ms: Option<u64>,
    /// Checkpoint sequence number. Set for streamed/checkpointed transactions, None for mutations.
    pub cp_sequence_number: Option<u64>,
}

impl KvArgs {
    /// `max_batch_get_transactions`/`max_batch_get_objects` may lower
    /// `LedgerGrpcReader`'s batch-chunking size below `MAX_BATCH_GET_TRANSACTIONS`/
    /// `MAX_BATCH_GET_OBJECTS`, never raise it above — a larger value would just be
    /// rejected by the ledger gRPC/KV-RPC service, so it's clamped rather
    /// than passed through.
    pub async fn ledger_grpc_reader(
        &self,
        prefix: Option<&str>,
        registry: &Registry,
        max_batch_get_transactions: Option<usize>,
        max_batch_get_objects: Option<usize>,
    ) -> anyhow::Result<Option<LedgerGrpcReader>> {
        let Some(ledger_grpc_url) = self.ledger_grpc_url.as_ref() else {
            return Ok(None);
        };

        Ok(Some(
            LedgerGrpcReader::new(
                ledger_grpc_url.clone(),
                self.ledger_grpc_args(),
                prefix,
                registry,
                max_batch_get_transactions
                    .unwrap_or(MAX_BATCH_GET_TRANSACTIONS)
                    .min(MAX_BATCH_GET_TRANSACTIONS),
                max_batch_get_objects
                    .unwrap_or(MAX_BATCH_GET_OBJECTS)
                    .min(MAX_BATCH_GET_OBJECTS),
            )
            .await?,
        ))
    }

    /// Construct a streaming list reader when the operator has opted in via
    /// `enable_list_apis` AND a ledger gRPC URL is configured. Returns `None`
    /// otherwise. Reuses the same channel settings as the v2 `ledger_grpc_reader`.
    pub async fn alpha_ledger_grpc_reader(
        &self,
        prefix: Option<&str>,
        registry: &Registry,
    ) -> anyhow::Result<Option<AlphaLedgerGrpcReader>> {
        if !self.enable_list_apis.unwrap_or(false) {
            return Ok(None);
        }
        let Some(ledger_grpc_url) = self.ledger_grpc_url.as_ref() else {
            return Ok(None);
        };

        Ok(Some(
            AlphaLedgerGrpcReader::new(
                ledger_grpc_url.clone(),
                self.ledger_grpc_args(),
                prefix,
                registry,
            )
            .await?,
        ))
    }

    fn ledger_grpc_args(&self) -> LedgerGrpcArgs {
        LedgerGrpcArgs::new(
            self.kv_statement_timeout_ms,
            self.kv_max_decoding_message_size,
        )
    }
}

impl KvLoader {
    pub fn new(ledger_grpc: LedgerGrpcReader) -> Self {
        Self(Arc::new(ledger_grpc.as_data_loader()))
    }

    pub async fn load_one_object(
        &self,
        id: ObjectID,
        version: u64,
    ) -> Result<Option<Object>, Error> {
        self.0.load_one(VersionedObjectKey(id, version)).await
    }

    pub async fn load_many_objects(
        &self,
        keys: Vec<VersionedObjectKey>,
    ) -> Result<HashMap<VersionedObjectKey, Object>, Error> {
        self.0.load_many(keys).await
    }

    pub async fn load_one_checkpoint(
        &self,
        sequence_number: u64,
    ) -> Result<
        Option<(
            CheckpointSummary,
            CheckpointContents,
            AuthorityQuorumSignInfo<true>,
        )>,
        Error,
    > {
        self.0.load_one(CheckpointKey(sequence_number)).await
    }

    /// Resolve a checkpoint digest to its sequence number. Returns `None` if the digest is not
    /// found. Used by `Query.checkpoint(digest:)` to translate a caller-supplied digest into the
    /// sequence number that downstream resolvers consume.
    ///
    /// Calls the reader directly rather than through the `DataLoader`, since the ledger gRPC
    /// backend only supports single-digest lookup — `DataLoader` can't add real batching here; it
    /// would only fan keys out into N parallel backend requests.
    pub async fn load_one_checkpoint_seq_by_digest(
        &self,
        digest: CheckpointDigest,
    ) -> Result<Option<u64>, Error> {
        self.0
            .loader()
            .checkpoint_seq_by_digest(digest)
            .await
            .map_err(Error::from)
    }

    pub async fn load_one_transaction(
        &self,
        digest: TransactionDigest,
    ) -> Result<Option<TransactionContents>, Error> {
        Ok(self
            .0
            .load_one(TransactionKey(digest))
            .await?
            .map(TransactionContents::LedgerGrpc))
    }

    pub async fn load_one_transaction_timestamp(
        &self,
        digest: TransactionDigest,
    ) -> Result<Option<u64>, Error> {
        self.0.load_one(TransactionTimestampKey(digest)).await
    }

    /// Load a transaction's effects as rendered by the ledger service.
    pub async fn load_one_rendered_effects(
        &self,
        digest: TransactionDigest,
    ) -> Result<Option<grpc::TransactionEffects>, Error> {
        self.0.load_one(ProtoEffectsKey(digest)).await
    }

    pub async fn load_many_transaction_events(
        &self,
        digests: Vec<TransactionDigest>,
    ) -> Result<HashMap<TransactionDigest, grpc::ExecutedTransaction>, Arc<Error>> {
        let keys = digests
            .iter()
            .map(|d| TransactionEventsKey(*d))
            .collect::<Vec<_>>();

        Ok(self
            .0
            .load_many(keys)
            .await?
            .into_iter()
            .map(|(key, data)| (key.0, data))
            .collect())
    }

    pub async fn load_many_transactions(
        &self,
        digests: Vec<TransactionDigest>,
    ) -> Result<HashMap<TransactionDigest, TransactionContents>, Arc<Error>> {
        let keys = digests
            .iter()
            .map(|d| TransactionKey(*d))
            .collect::<Vec<_>>();

        Ok(self
            .0
            .load_many(keys)
            .await?
            .into_iter()
            .map(|(key, txn)| (key.0, TransactionContents::LedgerGrpc(txn)))
            .collect())
    }
}

impl TransactionContents {
    pub fn from_executed_transaction(
        executed_transaction: &grpc::ExecutedTransaction,
        transaction_data: TransactionData,
        signatures: Vec<GenericSignature>,
    ) -> anyhow::Result<Self> {
        // Parse effects from BCS
        let effects: TransactionEffects = executed_transaction
            .effects
            .as_ref()
            .and_then(|effects| effects.bcs.as_ref())
            .context("Effects BCS should be present")?
            .deserialize()
            .context("Effects BCS should be valid")?;

        // Parse events from BCS if present, defaulting to empty when absent.
        let events: Vec<Arc<Event>> = executed_transaction
            .events
            .as_ref()
            .and_then(|events| events.bcs.as_ref())
            .map(|bcs| bcs.deserialize().context("Events BCS should be valid"))
            .transpose()?
            .map(|events: TransactionEvents| events.data.into_iter().map(Arc::new).collect())
            .unwrap_or_default();

        let balance_changes = executed_transaction.balance_changes.clone();

        // Store the proto effects and transaction for JSON serialization
        let proto_effects = executed_transaction.effects.clone();
        let proto_transaction = executed_transaction.transaction.clone();

        Ok(Self::ExecutedTransaction(ExecutedTransactionData {
            effects: Box::new(effects),
            events,
            transaction_data: Box::new(transaction_data),
            signatures,
            balance_changes,
            proto_effects,
            proto_transaction,
            timestamp_ms: None,
            cp_sequence_number: None,
        }))
    }

    /// A minimal instance whose `digest()` returns `digest`, for tests that only need identity. All
    /// other accessors resolve to empty or absent values.
    #[cfg(feature = "testing")]
    pub fn for_test(digest: TransactionDigest) -> Self {
        let mut effects = TransactionEffects::default();
        *effects.transaction_digest_mut_for_testing() = digest;

        let pt = sui_types::programmable_transaction_builder::ProgrammableTransactionBuilder::new()
            .finish();
        let transaction_data = TransactionData::new_programmable(
            sui_types::base_types::SuiAddress::ZERO,
            vec![],
            pt,
            0,
            0,
        );

        Self::ExecutedTransaction(ExecutedTransactionData {
            effects: Box::new(effects),
            events: vec![],
            transaction_data: Box::new(transaction_data),
            signatures: vec![],
            balance_changes: vec![],
            proto_effects: None,
            proto_transaction: None,
            timestamp_ms: None,
            cp_sequence_number: None,
        })
    }

    pub fn data(&self) -> anyhow::Result<TransactionData> {
        match self {
            Self::LedgerGrpc(txn) => Ok(txn.transaction_data.as_ref().clone()),
            Self::ExecutedTransaction(tx) => Ok(tx.transaction_data.as_ref().clone()),
        }
    }

    pub fn digest(&self) -> anyhow::Result<TransactionDigest> {
        match self {
            Self::LedgerGrpc(txn) => Ok(*txn.effects.as_ref().transaction_digest()),
            Self::ExecutedTransaction(tx) => Ok(*tx.effects.as_ref().transaction_digest()),
        }
    }

    pub fn effects_digest(&self) -> anyhow::Result<TransactionEffectsDigest> {
        match self {
            Self::LedgerGrpc(txn) => Ok(txn.effects.digest()),
            Self::ExecutedTransaction(tx) => Ok(tx.effects.digest()),
        }
    }

    pub fn signatures(&self) -> anyhow::Result<Vec<GenericSignature>> {
        match self {
            Self::LedgerGrpc(txn) => Ok(txn.signatures.clone()),
            Self::ExecutedTransaction(tx) => Ok(tx.signatures.clone()),
        }
    }

    pub fn effects(&self) -> anyhow::Result<TransactionEffects> {
        match self {
            Self::LedgerGrpc(txn) => Ok(txn.effects.as_ref().clone()),
            Self::ExecutedTransaction(tx) => Ok(tx.effects.as_ref().clone()),
        }
    }

    /// Returns the events for this transaction. Each `Event` is wrapped in an `Arc` so
    /// callers fanning the same transaction out to multiple consumers (e.g., subscription
    /// resolvers serving different subscribers) share the underlying event allocation
    /// rather than each performing a deep clone.
    pub fn events(&self) -> anyhow::Result<Vec<Arc<Event>>> {
        fn wrap(events: Vec<Event>) -> Vec<Arc<Event>> {
            events.into_iter().map(Arc::new).collect()
        }
        match self {
            Self::LedgerGrpc(txn) => Ok(wrap(txn.events.clone().unwrap_or_default())),
            Self::ExecutedTransaction(tx) => Ok(tx.events.clone()),
        }
    }

    pub fn balance_changes(&self) -> &[grpc::BalanceChange] {
        match self {
            Self::ExecutedTransaction(tx) => &tx.balance_changes,
            Self::LedgerGrpc(txn) => &txn.balance_changes,
        }
    }

    /// The proto TransactionEffects cached from a gRPC execution or streaming response, if any.
    pub fn cached_proto_effects(&self) -> Option<&grpc::TransactionEffects> {
        match self {
            Self::ExecutedTransaction(tx) => tx.proto_effects.as_ref(),
            Self::LedgerGrpc(_) => None,
        }
    }

    /// Returns the proto TransactionEffects.
    ///
    /// Prefers the proto cached from an execution or streaming response (rendered by the fullnode).
    /// Otherwise retrieve the rendered effects from kv.
    pub async fn proto_effects(
        &self,
        kv_loader: &KvLoader,
    ) -> anyhow::Result<grpc::TransactionEffects> {
        if let Some(proto) = self.cached_proto_effects() {
            return Ok(proto.clone());
        }

        // TODO: fullnode-rendered protos also include clever error rendering, which sui-kv-rpc does
        // not implement (though it has the package resolver required).

        match kv_loader
            .load_one_rendered_effects(self.digest()?)
            .await
            .context("Failed to fetch rendered effects")?
        {
            Some(proto) => Ok(proto),
            None => Ok(self.effects()?.into()),
        }
    }

    /// Returns the proto Transaction.
    ///
    /// For ExecutedTransaction, returns the cached proto from gRPC.
    /// For other sources, converts native transaction to proto.
    pub fn proto_transaction(&self) -> anyhow::Result<grpc::Transaction> {
        match self {
            Self::ExecutedTransaction(tx) => {
                // Use cached proto if available, otherwise convert from native
                if let Some(proto) = &tx.proto_transaction {
                    Ok(proto.clone())
                } else {
                    Ok(self.data()?.into())
                }
            }
            Self::LedgerGrpc(_) => Ok(self.data()?.into()),
        }
    }

    pub fn raw_transaction(&self) -> anyhow::Result<Vec<u8>> {
        match self {
            Self::LedgerGrpc(txn) => bcs::to_bytes(txn.transaction_data.as_ref())
                .context("Failed to serialize transaction"),
            Self::ExecutedTransaction(tx) => bcs::to_bytes(tx.transaction_data.as_ref())
                .context("Failed to serialize transaction"),
        }
    }

    pub fn raw_effects(&self) -> anyhow::Result<Vec<u8>> {
        match self {
            Self::LedgerGrpc(txn) => {
                bcs::to_bytes(txn.effects.as_ref()).context("Failed to serialize effects")
            }
            Self::ExecutedTransaction(tx) => {
                bcs::to_bytes(tx.effects.as_ref()).context("Failed to serialize effects")
            }
        }
    }

    pub fn timestamp_ms(&self) -> Option<u64> {
        match self {
            Self::LedgerGrpc(txn) => txn.timestamp_ms,
            Self::ExecutedTransaction(tx) => tx.timestamp_ms,
        }
    }

    pub fn cp_sequence_number(&self) -> Option<u64> {
        match self {
            Self::LedgerGrpc(txn) => txn.cp_sequence_number,
            Self::ExecutedTransaction(tx) => tx.cp_sequence_number,
        }
    }
}
