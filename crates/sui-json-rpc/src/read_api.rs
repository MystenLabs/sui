// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use std::collections::HashMap;
use std::sync::Arc;
use std::time::Duration;

use anyhow::anyhow;
use async_trait::async_trait;
use backoff::future::retry;
use backoff::ExponentialBackoff;
use fastcrypto::encoding::Base64;
use fastcrypto_zkp::bn254::zk_login_api::ZkLoginEnv;
use futures::future::join_all;
use im::hashmap::HashMap as ImHashMap;
use indexmap::map::IndexMap;
use itertools::Itertools;
use jsonrpsee::core::RpcResult;
use jsonrpsee::RpcModule;
use move_bytecode_utils::module_cache::GetModule;
use move_core_types::annotated_value::{MoveStruct, MoveStructLayout, MoveValue};
use move_core_types::language_storage::StructTag;
use shared_crypto::intent::{IntentMessage, PersonalMessage};
use sui_json_rpc_types::ZkLoginIntentScope;
use sui_types::base_types::SuiAddress;
use sui_types::signature::{GenericSignature, VerifyParams};
use sui_types::signature_verification::VerifiedDigestCache;
use tap::TapFallible;
use tracing::{debug, error, info, instrument, trace, warn};

use mysten_metrics::add_server_timing;
use mysten_metrics::spawn_monitored_task;
use sui_core::authority::AuthorityState;
use sui_json_rpc_api::{
    validate_limit, JsonRpcMetrics, ReadApiOpenRpc, ReadApiServer, QUERY_MAX_RESULT_LIMIT,
    QUERY_MAX_RESULT_LIMIT_CHECKPOINTS,
};
use sui_json_rpc_types::{
    BalanceChange, Checkpoint, CheckpointId, CheckpointPage, DisplayFieldsResponse, EventFilter,
    ObjectChange, ProtocolConfigResponse, SuiEvent, SuiGetPastObjectRequest, SuiMoveStruct,
    SuiMoveValue, SuiMoveVariant, SuiObjectDataOptions, SuiObjectResponse, SuiPastObjectResponse,
    SuiTransactionBlock, SuiTransactionBlockEvents, SuiTransactionBlockResponse,
    SuiTransactionBlockResponseOptions,
};
use sui_open_rpc::Module;
use sui_protocol_config::{ProtocolConfig, ProtocolVersion};
use sui_storage::key_value_store::TransactionKeyValueStore;
use sui_types::base_types::{ObjectID, SequenceNumber, TransactionDigest};
use sui_types::collection_types::VecMap;
use sui_types::crypto::AggregateAuthoritySignature;
use sui_types::display::DisplayVersionUpdatedEvent;
use sui_types::effects::{TransactionEffects, TransactionEffectsAPI, TransactionEvents};
use sui_types::error::{SuiError, SuiObjectResponseError};
use sui_types::messages_checkpoint::{
    CheckpointContents, CheckpointSequenceNumber, CheckpointSummary, CheckpointTimestamp,
};
use sui_types::object::{Object, ObjectRead, PastObjectRead};
use sui_types::sui_serde::BigInt;
use sui_types::transaction::TransactionDataAPI;
use sui_types::transaction::{Transaction, TransactionData};

use crate::authority_state::{StateRead, StateReadError, StateReadResult};
use crate::error::{Error, RpcInterimResult, SuiRpcInputError};
use crate::{
    get_balance_changes_from_effect, get_object_changes, ObjectProviderCache, SuiRpcModule,
};
use crate::{with_tracing, ObjectProvider};
use fastcrypto::encoding::Encoding;
use fastcrypto::traits::ToFromBytes;
use shared_crypto::intent::Intent;
use sui_json_rpc_types::ZkLoginVerifyResult;
use sui_types::authenticator_state::{get_authenticator_state, ActiveJwk};
const MAX_DISPLAY_NESTED_LEVEL: usize = 10;

// An implementation of the read portion of the JSON-RPC interface intended for use in
// Fullnodes.
#[derive(Clone)]
pub struct ReadApi {
    pub state: Arc<dyn StateRead>,
    pub transaction_kv_store: Arc<TransactionKeyValueStore>,
    pub metrics: Arc<JsonRpcMetrics>,
}

// Internal data structure to make it easy to work with data returned from
// authority store and also enable code sharing between get_transaction_with_options,
// multi_get_transaction_with_options, etc.
#[derive(Default)]
struct IntermediateTransactionResponse {
    digest: TransactionDigest,
    transaction: Option<Transaction>,
    effects: Option<TransactionEffects>,
    events: Option<SuiTransactionBlockEvents>,
    checkpoint_seq: Option<CheckpointSequenceNumber>,
    balance_changes: Option<Vec<BalanceChange>>,
    object_changes: Option<Vec<ObjectChange>>,
    timestamp: Option<CheckpointTimestamp>,
    errors: Vec<String>,
}

impl IntermediateTransactionResponse {
    pub fn new(digest: TransactionDigest) -> Self {
        Self {
            digest,
            ..Default::default()
        }
    }

    pub fn transaction(&self) -> &Option<Transaction> {
        &self.transaction
    }
}

impl ReadApi {
    pub fn new(
        state: Arc<AuthorityState>,
        transaction_kv_store: Arc<TransactionKeyValueStore>,
        metrics: Arc<JsonRpcMetrics>,
    ) -> Self {
        Self {
            state,
            transaction_kv_store,
            metrics,
        }
    }

    async fn get_checkpoint_internal(&self, id: CheckpointId) -> Result<Checkpoint, Error> {
        Ok(match id {
            CheckpointId::SequenceNumber(seq) => {
                let verified_summary = self
                    .transaction_kv_store
                    .get_checkpoint_summary(seq)
                    .await?;
                let content = self
                    .transaction_kv_store
                    .get_checkpoint_contents(verified_summary.sequence_number)
                    .await?;
                let signature = verified_summary.auth_sig().signature.clone();
                (verified_summary.into_data(), content, signature).into()
            }
            CheckpointId::Digest(digest) => {
                let verified_summary = self
                    .transaction_kv_store
                    .get_checkpoint_summary_by_digest(digest)
                    .await?;
                let content = self
                    .transaction_kv_store
                    .get_checkpoint_contents(verified_summary.sequence_number)
                    .await?;
                let signature = verified_summary.auth_sig().signature.clone();
                (verified_summary.into_data(), content, signature).into()
            }
        })
    }

    pub async fn get_checkpoints_internal(
        state: Arc<dyn StateRead>,
        transaction_kv_store: Arc<TransactionKeyValueStore>,
        // If `Some`, the query will start from the next item after the specified cursor
        cursor: Option<CheckpointSequenceNumber>,
        limit: u64,
        descending_order: bool,
    ) -> StateReadResult<Vec<Checkpoint>> {
        let max_checkpoint = state.get_latest_checkpoint_sequence_number()?;
        let checkpoint_numbers =
            calculate_checkpoint_numbers(cursor, limit, descending_order, max_checkpoint);

        let verified_checkpoints = transaction_kv_store
            .multi_get_checkpoints_summaries(&checkpoint_numbers)
            .await?;

        let checkpoint_summaries_and_signatures: Vec<(
            CheckpointSummary,
            AggregateAuthoritySignature,
        )> = verified_checkpoints
            .into_iter()
            .flatten()
            .map(|check| {
                (
                    check.clone().into_summary_and_sequence().1,
                    check.get_validator_signature(),
                )
            })
            .collect();

        let checkpoint_contents = transaction_kv_store
            .multi_get_checkpoints_contents(&checkpoint_numbers)
            .await?;
        let contents: Vec<CheckpointContents> = checkpoint_contents.into_iter().flatten().collect();

        let mut checkpoints: Vec<Checkpoint> = vec![];

        for (summary_and_sig, content) in checkpoint_summaries_and_signatures
            .into_iter()
            .zip(contents.into_iter())
        {
            checkpoints.push(Checkpoint::from((
                summary_and_sig.0,
                content,
                summary_and_sig.1,
            )));
        }

        Ok(checkpoints)
    }

    #[instrument(skip_all)]
    async fn multi_get_transaction_blocks_internal(
        &self,
        digests: Vec<TransactionDigest>,
        opts: Option<SuiTransactionBlockResponseOptions>,
    ) -> Result<Vec<SuiTransactionBlockResponse>, Error> {
        trace!("start");

        let num_digests = digests.len();
        if num_digests > *QUERY_MAX_RESULT_LIMIT {
            Err(SuiRpcInputError::SizeLimitExceeded(
                QUERY_MAX_RESULT_LIMIT.to_string(),
            ))?
        }
        self.metrics
            .get_tx_blocks_limit
            .observe(digests.len() as f64);

        let opts = opts.unwrap_or_default();

        // use LinkedHashMap to dedup and can iterate in insertion order.
        let mut temp_response: IndexMap<&TransactionDigest, IntermediateTransactionResponse> =
            IndexMap::from_iter(
                digests
                    .iter()
                    .map(|k| (k, IntermediateTransactionResponse::new(*k))),
            );
        if temp_response.len() < num_digests {
            Err(SuiRpcInputError::ContainsDuplicates)?
        }

        if opts.require_input() {
            trace!("getting input");
            let digests_clone = digests.clone();
            let transactions =
                self.transaction_kv_store.multi_get_tx(&digests_clone).await.tap_err(
                    |err| debug!(digests=?digests_clone, "Failed to multi get transactions: {:?}", err),
                )?;

            for ((_digest, cache_entry), txn) in
                temp_response.iter_mut().zip(transactions.into_iter())
            {
                cache_entry.transaction = txn;
            }
        }

        // Fetch effects when `show_events` is true because events relies on effects
        if opts.require_effects() {
            trace!("getting effects");
            let digests_clone = digests.clone();
            let effects_list = self.transaction_kv_store
                .multi_get_fx_by_tx_digest(&digests_clone)
                .await
                .tap_err(
                    |err| debug!(digests=?digests_clone, "Failed to multi get effects for transactions: {:?}", err),
                )?;
            for ((_digest, cache_entry), e) in
                temp_response.iter_mut().zip(effects_list.into_iter())
            {
                cache_entry.effects = e;
            }
        }

        trace!("getting checkpoint sequence numbers");
        let checkpoint_seq_list = self
            .transaction_kv_store
            .multi_get_transaction_checkpoint(&digests)
            .await
            .tap_err(
                |err| debug!(digests=?digests, "Failed to multi get checkpoint sequence number: {:?}", err))?;
        for ((_digest, cache_entry), seq) in temp_response
            .iter_mut()
            .zip(checkpoint_seq_list.into_iter())
        {
            cache_entry.checkpoint_seq = seq;
        }

        let unique_checkpoint_numbers = temp_response
            .values()
            .filter_map(|cache_entry| cache_entry.checkpoint_seq.map(<u64>::from))
            // It's likely that many transactions have the same checkpoint, so we don't
            // need to over-fetch
            .unique()
            .collect::<Vec<CheckpointSequenceNumber>>();

        // fetch timestamp from the DB
        trace!("getting checkpoint summaries");
        let timestamps = self
            .transaction_kv_store
            .multi_get_checkpoints_summaries(&unique_checkpoint_numbers)
            .await
            .map_err(|e| {
                Error::UnexpectedError(format!("Failed to fetch checkpoint summaries by these checkpoint ids: {unique_checkpoint_numbers:?} with error: {e:?}"))
            })?
            .into_iter()
            .map(|c| c.map(|checkpoint| checkpoint.timestamp_ms));

        // construct a hashmap of checkpoint -> timestamp for fast lookup
        let checkpoint_to_timestamp = unique_checkpoint_numbers
            .into_iter()
            .zip(timestamps)
            .collect::<HashMap<_, _>>();

        // fill cache with the timestamp
        for (_, cache_entry) in temp_response.iter_mut() {
            if cache_entry.checkpoint_seq.is_some() {
                // safe to unwrap because is_some is checked
                cache_entry.timestamp = *checkpoint_to_timestamp
                    .get(
                        cache_entry
                            .checkpoint_seq
                            .map(<u64>::from)
                            .as_ref()
                            .unwrap(),
                    )
                    // Safe to unwrap because checkpoint_seq is guaranteed to exist in checkpoint_to_timestamp
                    .unwrap();
            }
        }

        if opts.show_events {
            trace!("getting events");
            let mut non_empty_digests = vec![];
            for cache_entry in temp_response.values() {
                if let Some(effects) = &cache_entry.effects {
                    if effects.events_digest().is_some() {
                        non_empty_digests.push(cache_entry.digest);
                    }
                }
            }
            // fetch events from the DB with retry, retry each 0.5s for 3s
            let backoff = ExponentialBackoff {
                max_elapsed_time: Some(Duration::from_secs(3)),
                multiplier: 1.0,
                ..ExponentialBackoff::default()
            };
            let mut events = retry(backoff, || async {
                match self
                    .transaction_kv_store
                    .multi_get_events_by_tx_digests(&non_empty_digests)
                    .await
                {
                    // Only return Ok when all the queried transaction events are found, otherwise retry
                    // until timeout, then return Err.
                    Ok(events) if !events.contains(&None) => Ok(events),
                    Ok(_) => Err(backoff::Error::transient(Error::UnexpectedError(
                        "Events not found, transaction execution may be incomplete.".into(),
                    ))),
                    Err(e) => Err(backoff::Error::permanent(Error::UnexpectedError(format!(
                        "Failed to call multi_get_events: {e:?}"
                    )))),
                }
            })
            .await
            .map_err(|e| {
                Error::UnexpectedError(format!(
                "Retrieving events with retry failed for transaction digests {digests:?}: {e:?}"
            ))
            })?
            .into_iter();

            // fill cache with the events
            for (_, cache_entry) in temp_response.iter_mut() {
                let transaction_digest = cache_entry.digest;
                if let Some(events_digest) =
                    cache_entry.effects.as_ref().and_then(|e| e.events_digest())
                {
                    match events.next() {
                        Some(Some(ev)) => {
                            cache_entry.events =
                                Some(to_sui_transaction_events(self, cache_entry.digest, ev)?)
                        }
                        None | Some(None) => {
                            error!("Failed to fetch events with event digest {events_digest:?} for txn {transaction_digest}");
                            cache_entry.errors.push(format!(
                                "Failed to fetch events with event digest {events_digest:?}",
                            ))
                        }
                    }
                } else {
                    // events field will be Some if and only if `show_events` is true and
                    // there is no error in converting fetching events
                    cache_entry.events = Some(SuiTransactionBlockEvents::default());
                }
            }
        }

        let object_cache =
            ObjectProviderCache::new((self.state.clone(), self.transaction_kv_store.clone()));
        if opts.show_balance_changes {
            trace!("getting balance changes");

            let mut results = vec![];
            for resp in temp_response.values() {
                let input_objects = if let Some(tx) = resp.transaction() {
                    tx.data()
                        .inner()
                        .intent_message
                        .value
                        .input_objects()
                        .unwrap_or_default()
                } else {
                    // don't have the input tx, so not much we can do. perhaps this is an Err?
                    Vec::new()
                };
                results.push(get_balance_changes_from_effect(
                    &object_cache,
                    resp.effects.as_ref().ok_or_else(|| {
                        SuiRpcInputError::GenericNotFound(
                            "unable to derive balance changes because effect is empty".to_string(),
                        )
                    })?,
                    input_objects,
                    None,
                ));
            }
            let results = join_all(results).await;
            for (result, entry) in results.into_iter().zip(temp_response.iter_mut()) {
                match result {
                    Ok(balance_changes) => entry.1.balance_changes = Some(balance_changes),
                    Err(e) => entry
                        .1
                        .errors
                        .push(format!("Failed to fetch balance changes {e:?}")),
                }
            }
        }

        if opts.show_object_changes {
            trace!("getting object changes");

            let mut results = vec![];
            for resp in temp_response.values() {
                let effects = resp.effects.as_ref().ok_or_else(|| {
                    SuiRpcInputError::GenericNotFound(
                        "unable to derive object changes because effect is empty".to_string(),
                    )
                })?;

                results.push(get_object_changes(
                    &object_cache,
                    effects,
                    resp.transaction
                        .as_ref()
                        .ok_or_else(|| {
                            SuiRpcInputError::GenericNotFound(
                                "unable to derive object changes because transaction is empty"
                                    .to_string(),
                            )
                        })?
                        .data()
                        .intent_message()
                        .value
                        .sender(),
                    effects.modified_at_versions(),
                    effects.all_changed_objects(),
                    effects.all_removed_objects(),
                ));
            }
            let results = join_all(results).await;
            for (result, entry) in results.into_iter().zip(temp_response.iter_mut()) {
                match result {
                    Ok(object_changes) => entry.1.object_changes = Some(object_changes),
                    Err(e) => entry
                        .1
                        .errors
                        .push(format!("Failed to fetch object changes {e:?}")),
                }
            }
        }

        let epoch_store = self.state.load_epoch_store_one_call_per_task();
        let converted_tx_block_resps = temp_response
            .into_iter()
            .map(|c| convert_to_response(c.1, &opts, epoch_store.module_cache()))
            .collect::<Result<Vec<_>, _>>()?;

        self.metrics
            .get_tx_blocks_result_size
            .observe(converted_tx_block_resps.len() as f64);
        self.metrics
            .get_tx_blocks_result_size_total
            .inc_by(converted_tx_block_resps.len() as u64);

        trace!("done");

        Ok(converted_tx_block_resps)
    }
}

#[async_trait]
impl ReadApiServer for ReadApi {
    #[instrument(skip(self))]
    async fn get_object(
        &self,
        object_id: ObjectID,
        options: Option<SuiObjectDataOptions>,
    ) -> RpcResult<SuiObjectResponse> {
        with_tracing!(async move {
            let state = self.state.clone();
            let object_read = spawn_monitored_task!(async move {
                state.get_object_read(&object_id).map_err(|e| {
                    warn!(?object_id, "Failed to get object: {:?}", e);
                    Error::from(e)
                })
            })
            .await
            .map_err(Error::from)??;
            let options = options.unwrap_or_default();

            match object_read {
                ObjectRead::NotExists(id) => Ok(SuiObjectResponse::new_with_error(
                    SuiObjectResponseError::NotExists { object_id: id },
                )),
                ObjectRead::Exists(object_ref, o, layout) => {
                    let mut display_fields = None;
                    if options.show_display {
                        match get_display_fields(self, &self.transaction_kv_store, &o, &layout)
                            .await
                        {
                            Ok(rendered_fields) => display_fields = Some(rendered_fields),
                            Err(e) => {
                                return Ok(SuiObjectResponse::new(
                                    Some((object_ref, o, layout, options, None).try_into()?),
                                    Some(SuiObjectResponseError::DisplayError {
                                        error: e.to_string(),
                                    }),
                                ));
                            }
                        }
                    }
                    Ok(SuiObjectResponse::new_with_data(
                        (object_ref, o, layout, options, display_fields).try_into()?,
                    ))
                }
                ObjectRead::Deleted((object_id, version, digest)) => Ok(
                    SuiObjectResponse::new_with_error(SuiObjectResponseError::Deleted {
                        object_id,
                        version,
                        digest,
                    }),
                ),
            }
        })
    }

    #[instrument(skip(self))]
    async fn multi_get_objects(
        &self,
        object_ids: Vec<ObjectID>,
        options: Option<SuiObjectDataOptions>,
    ) -> RpcResult<Vec<SuiObjectResponse>> {
        with_tracing!(async move {
            if object_ids.len() <= *QUERY_MAX_RESULT_LIMIT {
                self.metrics
                    .get_objects_limit
                    .observe(object_ids.len() as f64);
                let mut futures = vec![];
                for object_id in object_ids {
                    futures.push(self.get_object(object_id, options.clone()));
                }
                let results = join_all(futures).await;

                let objects_result: Result<Vec<SuiObjectResponse>, String> = results
                    .into_iter()
                    .map(|result| match result {
                        Ok(response) => Ok(response),
                        Err(error) => {
                            error!("Failed to fetch object with error: {error:?}");
                            Err(format!("Error: {}", error))
                        }
                    })
                    .collect();

                let objects = objects_result.map_err(|err| {
                    Error::UnexpectedError(format!("Failed to fetch objects with error: {}", err))
                })?;

                self.metrics
                    .get_objects_result_size
                    .observe(objects.len() as f64);
                self.metrics
                    .get_objects_result_size_total
                    .inc_by(objects.len() as u64);
                Ok(objects)
            } else {
                Err(SuiRpcInputError::SizeLimitExceeded(
                    QUERY_MAX_RESULT_LIMIT.to_string(),
                ))?
            }
        })
    }

    #[instrument(skip(self))]
    async fn try_get_past_object(
        &self,
        object_id: ObjectID,
        version: SequenceNumber,
        options: Option<SuiObjectDataOptions>,
    ) -> RpcResult<SuiPastObjectResponse> {
        with_tracing!(async move {
            let state = self.state.clone();
            let past_read = spawn_monitored_task!(async move {
            state.get_past_object_read(&object_id, version)
            .map_err(|e| {
                error!("Failed to call try_get_past_object for object: {object_id:?} version: {version:?} with error: {e:?}");
                Error::from(e)
            })}).await.map_err(Error::from)??;
            let options = options.unwrap_or_default();
            match past_read {
                PastObjectRead::ObjectNotExists(id) => {
                    Ok(SuiPastObjectResponse::ObjectNotExists(id))
                }
                PastObjectRead::VersionFound(object_ref, o, layout) => {
                    let display_fields = if options.show_display {
                        // TODO (jian): api breaking change to also modify past objects.
                        Some(
                            get_display_fields(self, &self.transaction_kv_store, &o, &layout)
                                .await
                                .map_err(|e| {
                                    Error::UnexpectedError(format!(
                                        "Unable to render object at version {version}: {e}"
                                    ))
                                })?,
                        )
                    } else {
                        None
                    };
                    Ok(SuiPastObjectResponse::VersionFound(
                        (object_ref, o, layout, options, display_fields).try_into()?,
                    ))
                }
                PastObjectRead::ObjectDeleted(oref) => {
                    Ok(SuiPastObjectResponse::ObjectDeleted(oref.into()))
                }
                PastObjectRead::VersionNotFound(id, seq_num) => {
                    Ok(SuiPastObjectResponse::VersionNotFound(id, seq_num))
                }
                PastObjectRead::VersionTooHigh {
                    object_id,
                    asked_version,
                    latest_version,
                } => Ok(SuiPastObjectResponse::VersionTooHigh {
                    object_id,
                    asked_version,
                    latest_version,
                }),
            }
        })
    }

    #[instrument(skip(self))]
    async fn try_get_object_before_version(
        &self,
        object_id: ObjectID,
        version: SequenceNumber,
    ) -> RpcResult<SuiPastObjectResponse> {
        let version = self
            .state
            .find_object_lt_or_eq_version(&object_id, &version)
            .await
            .map_err(Error::from)?
            .map(|obj| obj.version())
            .unwrap_or_default();
        self.try_get_past_object(
            object_id,
            version,
            Some(SuiObjectDataOptions::bcs_lossless()),
        )
        .await
    }

    #[instrument(skip(self))]
    async fn try_multi_get_past_objects(
        &self,
        past_objects: Vec<SuiGetPastObjectRequest>,
        options: Option<SuiObjectDataOptions>,
    ) -> RpcResult<Vec<SuiPastObjectResponse>> {
        with_tracing!(async move {
            if past_objects.len() <= *QUERY_MAX_RESULT_LIMIT {
                let mut futures = vec![];
                for past_object in past_objects {
                    futures.push(self.try_get_past_object(
                        past_object.object_id,
                        past_object.version,
                        options.clone(),
                    ));
                }
                let results = join_all(futures).await;

                let (oks, errs): (Vec<_>, Vec<_>) = results.into_iter().partition(Result::is_ok);
                let success = oks.into_iter().filter_map(Result::ok).collect();
                let errors: Vec<_> = errs.into_iter().filter_map(Result::err).collect();
                if !errors.is_empty() {
                    let error_string = errors
                        .iter()
                        .map(|e| e.to_string())
                        .collect::<Vec<String>>()
                        .join("; ");
                    Err(anyhow!("{error_string}").into()) // Collects errors not related to SuiPastObjectResponse variants
                } else {
                    Ok(success)
                }
            } else {
                Err(SuiRpcInputError::SizeLimitExceeded(
                    QUERY_MAX_RESULT_LIMIT.to_string(),
                ))?
            }
        })
    }

    #[instrument(skip(self))]
    async fn get_total_transaction_blocks(&self) -> RpcResult<BigInt<u64>> {
        with_tracing!(async move {
            Ok(self
                .state
                .get_total_transaction_blocks()
                .map_err(Error::from)?
                .into()) // converts into BigInt<u64>
        })
    }

    #[instrument(skip(self))]
    async fn get_transaction_block(
        &self,
        digest: TransactionDigest,
        opts: Option<SuiTransactionBlockResponseOptions>,
    ) -> RpcResult<SuiTransactionBlockResponse> {
        with_tracing!(async move {
            let opts = opts.unwrap_or_default();
            let mut temp_response = IntermediateTransactionResponse::new(digest);

            // Fetch transaction to determine existence
            let transaction_kv_store = self.transaction_kv_store.clone();
            let transaction = spawn_monitored_task!(async move {
                let ret = transaction_kv_store.get_tx(digest).await.map_err(|err| {
                    debug!(tx_digest=?digest, "Failed to get transaction: {:?}", err);
                    Error::from(err)
                });
                add_server_timing("tx_kv_lookup");
                ret
            })
            .await
            .map_err(Error::from)??;
            let input_objects = transaction
                .data()
                .inner()
                .intent_message
                .value
                .input_objects()
                .unwrap_or_default();

            // the input is needed for object_changes to retrieve the sender address.
            if opts.require_input() {
                temp_response.transaction = Some(transaction);
            }

            // Fetch effects when `show_events` is true because events relies on effects
            if opts.require_effects() {
                let transaction_kv_store = self.transaction_kv_store.clone();
                temp_response.effects = Some(
                    spawn_monitored_task!(async move {
                        transaction_kv_store
                            .get_fx_by_tx_digest(digest)
                            .await
                            .map_err(|err| {
                                debug!(tx_digest=?digest, "Failed to get effects: {:?}", err);
                                Error::from(err)
                            })
                    })
                    .await
                    .map_err(Error::from)??,
                );
            }

            temp_response.checkpoint_seq = self
                .transaction_kv_store
                .deprecated_get_transaction_checkpoint(digest)
                .await
                .map_err(|e| {
                    error!("Failed to retrieve checkpoint sequence for transaction {digest:?} with error: {e:?}");
                    Error::from(e)
                })?;

            if let Some(checkpoint_seq) = &temp_response.checkpoint_seq {
                let kv_store = self.transaction_kv_store.clone();
                let checkpoint_seq = *checkpoint_seq;
                let checkpoint = spawn_monitored_task!(async move {
                    kv_store
                    // safe to unwrap because we have checked `is_some` above
                    .get_checkpoint_summary(checkpoint_seq)
                    .await
                    .map_err(|e| {
                        error!("Failed to get checkpoint by sequence number: {checkpoint_seq:?} with error: {e:?}");
                        Error::from(e)
                    })
                }).await.map_err(Error::from)??;
                // TODO(chris): we don't need to fetch the whole checkpoint summary
                temp_response.timestamp = Some(checkpoint.timestamp_ms);
            }

            if opts.show_events && temp_response.effects.is_some() {
                let transaction_kv_store = self.transaction_kv_store.clone();
                let events = spawn_monitored_task!(async move {
                    transaction_kv_store
                        .multi_get_events_by_tx_digests(&[digest])
                        .await
                        .map_err(|e| {
                            error!("Failed to call get transaction events for transaction: {digest:?} with error {e:?}");
                            Error::from(e)
                        })
                    })
                    .await
                    .map_err(Error::from)??
                    .pop()
                    .flatten();
                match events {
                    None => temp_response.events = Some(SuiTransactionBlockEvents::default()),
                    Some(events) => match to_sui_transaction_events(self, digest, events) {
                        Ok(e) => temp_response.events = Some(e),
                        Err(e) => temp_response.errors.push(e.to_string()),
                    },
                }
            }

            let object_cache =
                ObjectProviderCache::new((self.state.clone(), self.transaction_kv_store.clone()));
            if opts.show_balance_changes {
                if let Some(effects) = &temp_response.effects {
                    let balance_changes = get_balance_changes_from_effect(
                        &object_cache,
                        effects,
                        input_objects,
                        None,
                    )
                    .await;

                    if let Ok(balance_changes) = balance_changes {
                        temp_response.balance_changes = Some(balance_changes);
                    } else {
                        temp_response.errors.push(format!(
                            "Cannot retrieve balance changes: {}",
                            balance_changes.unwrap_err()
                        ));
                    }
                }
            }

            if opts.show_object_changes {
                if let (Some(effects), Some(input)) =
                    (&temp_response.effects, &temp_response.transaction)
                {
                    let sender = input.data().intent_message().value.sender();
                    let object_changes = get_object_changes(
                        &object_cache,
                        effects,
                        sender,
                        effects.modified_at_versions(),
                        effects.all_changed_objects(),
                        effects.all_removed_objects(),
                    )
                    .await;

                    if let Ok(object_changes) = object_changes {
                        temp_response.object_changes = Some(object_changes);
                    } else {
                        temp_response.errors.push(format!(
                            "Cannot retrieve object changes: {}",
                            object_changes.unwrap_err()
                        ));
                    }
                }
            }
            let epoch_store = self.state.load_epoch_store_one_call_per_task();
            convert_to_response(temp_response, &opts, epoch_store.module_cache())
        })
    }

    #[instrument(skip(self))]
    async fn multi_get_transaction_blocks(
        &self,
        digests: Vec<TransactionDigest>,
        opts: Option<SuiTransactionBlockResponseOptions>,
    ) -> RpcResult<Vec<SuiTransactionBlockResponse>> {
        with_tracing!(async move {
            let cloned_self = self.clone();
            spawn_monitored_task!(async move {
                cloned_self
                    .multi_get_transaction_blocks_internal(digests, opts)
                    .await
            })
            .await
            .map_err(Error::from)?
        })
    }

    #[instrument(skip(self))]
    async fn get_events(&self, transaction_digest: TransactionDigest) -> RpcResult<Vec<SuiEvent>> {
        with_tracing!(async move {
            let state = self.state.clone();
            let transaction_kv_store = self.transaction_kv_store.clone();
            spawn_monitored_task!(async move{
            let store = state.load_epoch_store_one_call_per_task();
            let events = transaction_kv_store
                .multi_get_events_by_tx_digests(&[transaction_digest])
                .await
                .map_err(
                    |e| {
                        error!("Failed to get transaction events for transaction {transaction_digest:?} with error: {e:?}");
                        Error::StateReadError(e.into())
                    })?
                .pop()
                .flatten();
            Ok(match events {
                Some(events) => events
                    .data
                    .into_iter()
                    .enumerate()
                    .map(|(seq, e)| {
                        let layout = store.executor().type_layout_resolver(Box::new(&state.get_backing_package_store().as_ref())).get_annotated_layout(&e.type_)?;
                        SuiEvent::try_from(e, transaction_digest, seq as u64, None, layout)
                    })
                    .collect::<Result<Vec<_>, _>>()
                    .map_err(Error::SuiError)?,
                None => vec![],
            })
        }).await.map_err(Error::from)?
        })
    }

    #[instrument(skip(self))]
    async fn get_latest_checkpoint_sequence_number(&self) -> RpcResult<BigInt<u64>> {
        with_tracing!(async move {
            Ok(self
                .state
                .get_latest_checkpoint_sequence_number()
                .map_err(|e| {
                    SuiRpcInputError::GenericNotFound(format!(
                        "Latest checkpoint sequence number was not found with error :{e}"
                    ))
                })?
                .into())
        })
    }

    #[instrument(skip(self))]
    async fn get_checkpoint(&self, id: CheckpointId) -> RpcResult<Checkpoint> {
        with_tracing!(self.get_checkpoint_internal(id))
    }

    #[instrument(skip(self))]
    async fn get_checkpoints(
        &self,
        // If `Some`, the query will start from the next item after the specified cursor
        cursor: Option<BigInt<u64>>,
        limit: Option<usize>,
        descending_order: bool,
    ) -> RpcResult<CheckpointPage> {
        with_tracing!(async move {
            let limit = validate_limit(limit, QUERY_MAX_RESULT_LIMIT_CHECKPOINTS)
                .map_err(SuiRpcInputError::from)?;

            let state = self.state.clone();
            let kv_store = self.transaction_kv_store.clone();

            self.metrics.get_checkpoints_limit.observe(limit as f64);

            let mut data = spawn_monitored_task!(Self::get_checkpoints_internal(
                state,
                kv_store,
                cursor.map(|s| *s),
                limit as u64 + 1,
                descending_order,
            ))
            .await
            .map_err(Error::from)?
            .map_err(Error::from)?;

            let has_next_page = data.len() > limit;
            data.truncate(limit);

            let next_cursor = if has_next_page {
                data.last().cloned().map(|d| d.sequence_number.into())
            } else {
                None
            };

            self.metrics
                .get_checkpoints_result_size
                .observe(data.len() as f64);
            self.metrics
                .get_checkpoints_result_size_total
                .inc_by(data.len() as u64);

            Ok(CheckpointPage {
                data,
                next_cursor,
                has_next_page,
            })
        })
    }

    #[instrument(skip(self))]
    async fn get_protocol_config(
        &self,
        version: Option<BigInt<u64>>,
    ) -> RpcResult<ProtocolConfigResponse> {
        with_tracing!(async move {
            version
                .map(|v| {
                    ProtocolConfig::get_for_version_if_supported(
                        (*v).into(),
                        self.state.get_chain_identifier()?.chain(),
                    )
                    .ok_or(SuiRpcInputError::ProtocolVersionUnsupported(
                        ProtocolVersion::MIN.as_u64(),
                        ProtocolVersion::MAX.as_u64(),
                    ))
                    .map_err(Error::from)
                })
                .unwrap_or(Ok(self
                    .state
                    .load_epoch_store_one_call_per_task()
                    .protocol_config()
                    .clone()))
                .map(ProtocolConfigResponse::from)
        })
    }

    #[instrument(skip(self))]
    async fn get_chain_identifier(&self) -> RpcResult<String> {
        with_tracing!(async move {
            let ci = self.state.get_chain_identifier()?;
            Ok(ci.to_string())
        })
    }
    #[instrument(skip(self))]
    async fn verify_zklogin_signature(
        &self,
        bytes: String,
        signature: String,
        intent_scope: ZkLoginIntentScope,
        author: SuiAddress,
    ) -> RpcResult<ZkLoginVerifyResult> {
        let epoch_store = self.state.load_epoch_store_one_call_per_task();
        let curr_epoch = epoch_store.epoch();
        let zklogin_env_native = match self
            .state
            .get_chain_identifier()
            .expect("get chain identifier should not fail")
            .chain()
        {
            sui_protocol_config::Chain::Mainnet | sui_protocol_config::Chain::Testnet => {
                ZkLoginEnv::Prod
            }
            _ => ZkLoginEnv::Test,
        };
        let GenericSignature::ZkLoginAuthenticator(zklogin_sig) =
            GenericSignature::from_bytes(&Base64::decode(&signature).map_err(Error::from)?)
                .map_err(Error::from)?
        else {
            return Err(SuiRpcInputError::GenericNotFound(
                "Endpoint only supports zkLogin signature".to_string(),
            )
            .into());
        };

        let new_jwks =
            match get_authenticator_state(self.state.get_object_store()).map_err(Error::from)? {
                Some(authenticator_state) => authenticator_state.active_jwks,
                None => {
                    return Err(SuiRpcInputError::GenericNotFound(
                        "Authenticator state not found".to_string(),
                    )
                    .into());
                }
            };

        // construct verify params with active jwks and zklogin_env.
        let mut oidc_provider_jwks = ImHashMap::new();
        for active_jwk in new_jwks.iter() {
            let ActiveJwk { jwk_id, jwk, .. } = active_jwk;
            match oidc_provider_jwks.entry(jwk_id.clone()) {
                im::hashmap::Entry::Occupied(_) => {
                    warn!("JWK with kid {:?} already exists", jwk_id);
                }
                im::hashmap::Entry::Vacant(entry) => {
                    entry.insert(jwk.clone());
                }
            }
        }
        let verify_params = VerifyParams::new(
            oidc_provider_jwks,
            vec![],
            zklogin_env_native,
            true,
            true,
            Some(30),
        );
        match intent_scope {
            ZkLoginIntentScope::TransactionData => {
                let tx_data: TransactionData =
                    bcs::from_bytes(&Base64::decode(&bytes).map_err(Error::from)?)
                        .map_err(Error::from)?;
                let intent_msg = IntentMessage::new(Intent::sui_transaction(), tx_data.clone());
                let sig = GenericSignature::ZkLoginAuthenticator(zklogin_sig);
                match sig.verify_authenticator(
                    &intent_msg,
                    author,
                    curr_epoch,
                    &verify_params,
                    Arc::new(VerifiedDigestCache::new_empty()),
                ) {
                    Ok(_) => Ok(ZkLoginVerifyResult {
                        success: true,
                        errors: vec![],
                    }),
                    Err(e) => Ok(ZkLoginVerifyResult {
                        success: false,
                        errors: vec![e.to_string()],
                    }),
                }
            }
            ZkLoginIntentScope::PersonalMessage => {
                let data = PersonalMessage {
                    message: Base64::decode(&bytes).map_err(Error::from)?,
                };
                let intent_msg = IntentMessage::new(Intent::personal_message(), data);

                let sig = GenericSignature::ZkLoginAuthenticator(zklogin_sig);
                match sig.verify_authenticator(
                    &intent_msg,
                    author,
                    curr_epoch,
                    &verify_params,
                    Arc::new(VerifiedDigestCache::new_empty()),
                ) {
                    Ok(_) => Ok(ZkLoginVerifyResult {
                        success: true,
                        errors: vec![],
                    }),
                    Err(e) => Ok(ZkLoginVerifyResult {
                        success: false,
                        errors: vec![e.to_string()],
                    }),
                }
            }
        }
    }
}

impl SuiRpcModule for ReadApi {
    fn rpc(self) -> RpcModule<Self> {
        self.into_rpc()
    }

    fn rpc_doc_module() -> Module {
        ReadApiOpenRpc::module_doc()
    }
}

#[instrument(skip_all)]
fn to_sui_transaction_events(
    fullnode_api: &ReadApi,
    tx_digest: TransactionDigest,
    events: TransactionEvents,
) -> Result<SuiTransactionBlockEvents, Error> {
    let epoch_store = fullnode_api.state.load_epoch_store_one_call_per_task();
    let backing_package_store = fullnode_api.state.get_backing_package_store();
    let mut layout_resolver = epoch_store
        .executor()
        .type_layout_resolver(Box::new(backing_package_store.as_ref()));
    Ok(SuiTransactionBlockEvents::try_from(
        events,
        tx_digest,
        None,
        layout_resolver.as_mut(),
    )?)
}

#[derive(Debug, thiserror::Error)]
pub enum ObjectDisplayError {
    #[error("Not a move struct")]
    NotMoveStruct,

    #[error("Failed to extract layout")]
    Layout,

    #[error("Failed to extract Move object")]
    MoveObject,

    #[error(transparent)]
    Deserialization(#[from] SuiError),

    #[error("Failed to deserialize 'VersionUpdatedEvent': {0}")]
    Bcs(#[from] bcs::Error),

    #[error(transparent)]
    StateReadError(#[from] StateReadError),
}

#[instrument(skip(fullnode_api, kv_store))]
async fn get_display_fields(
    fullnode_api: &ReadApi,
    kv_store: &Arc<TransactionKeyValueStore>,
    original_object: &Object,
    original_layout: &Option<MoveStructLayout>,
) -> Result<DisplayFieldsResponse, ObjectDisplayError> {
    let Some((object_type, layout)) = get_object_type_and_struct(original_object, original_layout)?
    else {
        return Ok(DisplayFieldsResponse {
            data: None,
            error: None,
        });
    };
    if let Some(display_object) =
        get_display_object_by_type(kv_store, fullnode_api, &object_type).await?
    {
        return get_rendered_fields(display_object.fields, &layout);
    }
    Ok(DisplayFieldsResponse {
        data: None,
        error: None,
    })
}

#[instrument(skip(kv_store, fullnode_api))]
async fn get_display_object_by_type(
    kv_store: &Arc<TransactionKeyValueStore>,
    fullnode_api: &ReadApi,
    object_type: &StructTag,
    // TODO: add query version support
) -> Result<Option<DisplayVersionUpdatedEvent>, ObjectDisplayError> {
    let mut events = fullnode_api
        .state
        .query_events(
            kv_store,
            EventFilter::MoveEventType(DisplayVersionUpdatedEvent::type_(object_type)),
            None,
            1,
            true,
        )
        .await?;

    // If there's any recent version of Display, give it to the client.
    // TODO: add support for version query.
    if let Some(event) = events.pop() {
        let display: DisplayVersionUpdatedEvent = bcs::from_bytes(&event.bcs.into_bytes())?;
        Ok(Some(display))
    } else {
        Ok(None)
    }
}

pub fn get_object_type_and_struct(
    o: &Object,
    layout: &Option<MoveStructLayout>,
) -> Result<Option<(StructTag, MoveStruct)>, ObjectDisplayError> {
    if let Some(object_type) = o.type_() {
        let move_struct = get_move_struct(o, layout)?;
        Ok(Some((object_type.clone().into(), move_struct)))
    } else {
        Ok(None)
    }
}

fn get_move_struct(
    o: &Object,
    layout: &Option<MoveStructLayout>,
) -> Result<MoveStruct, ObjectDisplayError> {
    let layout = layout.as_ref().ok_or_else(|| ObjectDisplayError::Layout)?;
    Ok(o.data
        .try_as_move()
        .ok_or_else(|| ObjectDisplayError::MoveObject)?
        .to_move_struct(layout)?)
}

pub fn get_rendered_fields(
    fields: VecMap<String, String>,
    move_struct: &MoveStruct,
) -> Result<DisplayFieldsResponse, ObjectDisplayError> {
    let sui_move_value: SuiMoveValue = MoveValue::Struct(move_struct.clone()).into();
    if let SuiMoveValue::Struct(move_struct) = sui_move_value {
        let fields =
            fields
                .contents
                .iter()
                .map(|entry| match parse_template(&entry.value, &move_struct) {
                    Ok(value) => Ok((entry.key.clone(), value)),
                    Err(e) => Err(e),
                });
        let (oks, errs): (Vec<_>, Vec<_>) = fields.partition(Result::is_ok);
        let success = oks.into_iter().filter_map(Result::ok).collect();
        let errors: Vec<_> = errs.into_iter().filter_map(Result::err).collect();
        let error_string = errors
            .iter()
            .map(|e| e.to_string())
            .collect::<Vec<String>>()
            .join("; ");
        let error = if !error_string.is_empty() {
            Some(SuiObjectResponseError::DisplayError {
                error: anyhow!("{error_string}").to_string(),
            })
        } else {
            None
        };

        return Ok(DisplayFieldsResponse {
            data: Some(success),
            error,
        });
    }
    Err(ObjectDisplayError::NotMoveStruct)?
}

fn parse_template(template: &str, move_struct: &SuiMoveStruct) -> Result<String, Error> {
    let mut output = template.to_string();
    let mut var_name = String::new();
    let mut in_braces = false;
    let mut escaped = false;

    for ch in template.chars() {
        match ch {
            '\\' => {
                escaped = true;
                continue;
            }
            '{' if !escaped => {
                in_braces = true;
                var_name.clear();
            }
            '}' if !escaped => {
                in_braces = false;
                let value = get_value_from_move_struct(move_struct, &var_name)?;
                output = output.replace(&format!("{{{}}}", var_name), &value.to_string());
            }
            _ if !escaped => {
                if in_braces {
                    var_name.push(ch);
                }
            }
            _ => {}
        }
        escaped = false;
    }

    Ok(output.replace('\\', ""))
}

fn get_value_from_move_struct(
    move_struct: &SuiMoveStruct,
    var_name: &str,
) -> Result<String, Error> {
    let parts: Vec<&str> = var_name.split('.').collect();
    if parts.is_empty() {
        Err(anyhow!("Display template value cannot be empty"))?;
    }
    if parts.len() > MAX_DISPLAY_NESTED_LEVEL {
        Err(anyhow!(
            "Display template value nested depth cannot exist {}",
            MAX_DISPLAY_NESTED_LEVEL
        ))?;
    }
    let mut current_value = &SuiMoveValue::Struct(move_struct.clone());
    // iterate over the parts and try to access the corresponding field
    for part in parts {
        match current_value {
            SuiMoveValue::Struct(move_struct) => {
                if let SuiMoveStruct::WithTypes { type_: _, fields }
                | SuiMoveStruct::WithFields(fields) = move_struct
                {
                    if let Some(value) = fields.get(part) {
                        current_value = value;
                    } else {
                        Err(anyhow!(
                            "Field value {} cannot be found in struct",
                            var_name
                        ))?;
                    }
                } else {
                    Err(Error::UnexpectedError(format!(
                        "Unexpected move struct type for field {}",
                        var_name
                    )))?;
                }
            }
            SuiMoveValue::Variant(SuiMoveVariant {
                fields, variant, ..
            }) => {
                if let Some(value) = fields.get(part) {
                    current_value = value;
                } else {
                    Err(anyhow!(
                        "Field value {var_name} cannot be found in variant {variant}",
                    ))?
                }
            }
            _ => {
                return Err(Error::UnexpectedError(format!(
                    "Unexpected move value type for field {}",
                    var_name
                )))?
            }
        }
    }

    match current_value {
        SuiMoveValue::Option(move_option) => match move_option.as_ref() {
            Some(move_value) => Ok(move_value.to_string()),
            None => Ok("".to_string()),
        },
        SuiMoveValue::Vector(_) => Err(anyhow!(
            "Vector is not supported as a Display value {}",
            var_name
        ))?,

        _ => Ok(current_value.to_string()),
    }
}

#[instrument(skip_all)]
fn convert_to_response(
    cache: IntermediateTransactionResponse,
    opts: &SuiTransactionBlockResponseOptions,
    module_cache: &impl GetModule,
) -> RpcInterimResult<SuiTransactionBlockResponse> {
    let mut response = SuiTransactionBlockResponse::new(cache.digest);
    response.errors = cache.errors;

    if let Some(transaction) = cache.transaction {
        if opts.show_raw_input {
            response.raw_transaction = bcs::to_bytes(transaction.data()).map_err(|e| {
                // TODO: is this a client or server error?
                anyhow!("Failed to serialize raw transaction with error: {e}")
            })?;
        }

        if opts.show_input {
            response.transaction = Some(SuiTransactionBlock::try_from(
                transaction.into_data(),
                module_cache,
            )?);
        }
    }

    if let Some(effects) = cache.effects {
        if opts.show_raw_effects {
            response.raw_effects = bcs::to_bytes(&effects).map_err(|e| {
                // TODO: is this a client or server error?
                anyhow!("Failed to serialize transaction block effects with error: {e}")
            })?;
        }

        if opts.show_effects {
            response.effects = Some(effects.try_into().map_err(|e| {
                // TODO: is this a client or server error?
                anyhow!("Failed to convert transaction block effects with error: {e}")
            })?);
        }
    }

    response.checkpoint = cache.checkpoint_seq;
    response.timestamp_ms = cache.timestamp;

    if opts.show_events {
        response.events = cache.events;
    }

    if opts.show_balance_changes {
        response.balance_changes = cache.balance_changes;
    }

    if opts.show_object_changes {
        response.object_changes = cache.object_changes;
    }

    Ok(response)
}

fn calculate_checkpoint_numbers(
    // If `Some`, the query will start from the next item after the specified cursor
    cursor: Option<CheckpointSequenceNumber>,
    limit: u64,
    descending_order: bool,
    max_checkpoint: CheckpointSequenceNumber,
) -> Vec<CheckpointSequenceNumber> {
    let (start_index, end_index) = match cursor {
        Some(t) => {
            if descending_order {
                let start = std::cmp::min(t.saturating_sub(1), max_checkpoint);
                let end = start.saturating_sub(limit - 1);
                (end, start)
            } else {
                let start =
                    std::cmp::min(t.checked_add(1).unwrap_or(max_checkpoint), max_checkpoint);
                let end = std::cmp::min(
                    start.checked_add(limit - 1).unwrap_or(max_checkpoint),
                    max_checkpoint,
                );
                (start, end)
            }
        }
        None => {
            if descending_order {
                (max_checkpoint.saturating_sub(limit - 1), max_checkpoint)
            } else {
                (0, std::cmp::min(limit - 1, max_checkpoint))
            }
        }
    };

    if descending_order {
        (start_index..=end_index).rev().collect()
    } else {
        (start_index..=end_index).collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_calculate_checkpoint_numbers() {
        let cursor = Some(10);
        let limit = 5;
        let descending_order = true;
        let max_checkpoint = 15;

        let checkpoint_numbers =
            calculate_checkpoint_numbers(cursor, limit, descending_order, max_checkpoint);

        assert_eq!(checkpoint_numbers, vec![9, 8, 7, 6, 5]);
    }

    #[test]
    fn test_calculate_checkpoint_numbers_descending_no_cursor() {
        let cursor = None;
        let limit = 5;
        let descending_order = true;
        let max_checkpoint = 15;

        let checkpoint_numbers =
            calculate_checkpoint_numbers(cursor, limit, descending_order, max_checkpoint);

        assert_eq!(checkpoint_numbers, vec![15, 14, 13, 12, 11]);
    }

    #[test]
    fn test_calculate_checkpoint_numbers_ascending_no_cursor() {
        let cursor = None;
        let limit = 5;
        let descending_order = false;
        let max_checkpoint = 15;

        let checkpoint_numbers =
            calculate_checkpoint_numbers(cursor, limit, descending_order, max_checkpoint);

        assert_eq!(checkpoint_numbers, vec![0, 1, 2, 3, 4]);
    }

    #[test]
    fn test_calculate_checkpoint_numbers_ascending_with_cursor() {
        let cursor = Some(10);
        let limit = 5;
        let descending_order = false;
        let max_checkpoint = 15;

        let checkpoint_numbers =
            calculate_checkpoint_numbers(cursor, limit, descending_order, max_checkpoint);

        assert_eq!(checkpoint_numbers, vec![11, 12, 13, 14, 15]);
    }

    #[test]
    fn test_calculate_checkpoint_numbers_ascending_limit_exceeds_max() {
        let cursor = None;
        let limit = 20;
        let descending_order = false;
        let max_checkpoint = 15;

        let checkpoint_numbers =
            calculate_checkpoint_numbers(cursor, limit, descending_order, max_checkpoint);

        assert_eq!(checkpoint_numbers, (0..=15).collect::<Vec<_>>());
    }

    #[test]
    fn test_calculate_checkpoint_numbers_descending_limit_exceeds_max() {
        let cursor = None;
        let limit = 20;
        let descending_order = true;
        let max_checkpoint = 15;

        let checkpoint_numbers =
            calculate_checkpoint_numbers(cursor, limit, descending_order, max_checkpoint);

        assert_eq!(checkpoint_numbers, (0..=15).rev().collect::<Vec<_>>());
    }
}
