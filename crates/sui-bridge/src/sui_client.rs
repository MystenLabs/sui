// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use anyhow::anyhow;
use async_trait::async_trait;
use core::panic;
use fastcrypto::traits::ToFromBytes;
use std::collections::HashMap;
use std::str::from_utf8;
use std::sync::Arc;
use std::time::Duration;
use sui_json_rpc_types::BcsEvent;
use sui_json_rpc_types::{EventFilter, Page, SuiEvent};
use sui_json_rpc_types::{
    EventPage, SuiObjectDataOptions, SuiTransactionBlockResponse,
    SuiTransactionBlockResponseOptions,
};
use sui_rpc::field::{FieldMask, FieldMaskUtil};
use sui_rpc::proto::sui::rpc::v2::{
    Checkpoint, ExecutedTransaction, GetCheckpointRequest, GetObjectRequest, GetServiceInfoRequest,
    GetTransactionRequest, Object,
};
use sui_sdk::{SuiClient as SuiSdkClient, SuiClientBuilder};
use sui_sdk_types::Address;
use sui_types::BRIDGE_PACKAGE_ID;
use sui_types::SUI_BRIDGE_OBJECT_ID;
use sui_types::TypeTag;
use sui_types::base_types::ObjectRef;
use sui_types::base_types::SequenceNumber;
use sui_types::bridge::{
    BridgeSummary, BridgeWrapper, MoveTypeBridgeMessageKey, MoveTypeBridgeRecord,
};
use sui_types::bridge::{BridgeTrait, BridgeTreasurySummary};
use sui_types::bridge::{MoveTypeBridgeMessage, MoveTypeParsedTokenTransferMessage};
use sui_types::bridge::{MoveTypeCommitteeMember, MoveTypeTokenTransferPayload};
use sui_types::collection_types::LinkedTableNode;
use sui_types::gas_coin::GasCoin;
use sui_types::object::Owner;
use sui_types::parse_sui_type_tag;
use sui_types::transaction::ObjectArg;
use sui_types::transaction::SharedObjectMutability;
use sui_types::transaction::Transaction;
use sui_types::{Identifier, base_types::ObjectID, digests::TransactionDigest, event::EventID};
use tokio::sync::OnceCell;
use tracing::{error, warn};

use crate::crypto::BridgeAuthorityPublicKey;
use crate::error::{BridgeError, BridgeResult};
use crate::events::SuiBridgeEvent;
use crate::metrics::BridgeMetrics;
use crate::retry_with_max_elapsed_time;
use crate::types::BridgeActionStatus;
use crate::types::ParsedTokenTransferMessage;
use crate::types::SuiEvents;
use crate::types::{BridgeAction, BridgeAuthority, BridgeCommittee};

pub struct SuiClient<P> {
    inner: P,
    bridge_metrics: Arc<BridgeMetrics>,
}

pub type SuiBridgeClient = SuiClient<SuiClientInternal>;

pub struct SuiClientInternal {
    jsonrpc_client: SuiSdkClient,
    grpc_client: sui_rpc::Client,
}

impl SuiBridgeClient {
    pub async fn new(rpc_url: &str, bridge_metrics: Arc<BridgeMetrics>) -> anyhow::Result<Self> {
        let jsonrpc_client = SuiClientBuilder::default()
            .build(rpc_url)
            .await
            .map_err(|e| {
                anyhow!("Can't establish connection with Sui Rpc {rpc_url}. Error: {e}")
            })?;
        let grpc_client = sui_rpc::Client::new(rpc_url)?;
        let inner = SuiClientInternal {
            jsonrpc_client,
            grpc_client,
        };
        let self_ = Self {
            inner,
            bridge_metrics,
        };
        self_.describe().await.map_err(|e| anyhow::anyhow!("{e}"))?;
        Ok(self_)
    }

    pub fn jsonrpc_client(&self) -> &SuiSdkClient {
        &self.inner.jsonrpc_client
    }

    pub fn grpc_client(&self) -> &sui_rpc::Client {
        &self.inner.grpc_client
    }
}

impl<P> SuiClient<P>
where
    P: SuiClientInner,
{
    pub fn new_for_testing(inner: P) -> Self {
        Self {
            inner,
            bridge_metrics: Arc::new(BridgeMetrics::new_for_testing()),
        }
    }

    // TODO assert chain identifier
    async fn describe(&self) -> Result<(), BridgeError> {
        let chain_id = self.inner.get_chain_identifier().await?;
        let block_number = self.inner.get_latest_checkpoint_sequence_number().await?;
        tracing::info!(
            "SuiClient is connected to chain {chain_id}, current block number: {block_number}"
        );
        Ok(())
    }

    /// Get the mutable bridge object arg on chain.
    // We retry a few times in case of errors. If it fails eventually, we panic.
    // In general it's safe to call in the beginning of the program.
    // After the first call, the result is cached since the value should never change.
    pub async fn get_mutable_bridge_object_arg_must_succeed(&self) -> ObjectArg {
        static ARG: OnceCell<ObjectArg> = OnceCell::const_new();
        *ARG.get_or_init(|| async move {
            let Ok(Ok(bridge_object_arg)) = retry_with_max_elapsed_time!(
                self.inner.get_mutable_bridge_object_arg(),
                Duration::from_secs(30)
            ) else {
                panic!("Failed to get bridge object arg after retries");
            };
            bridge_object_arg
        })
        .await
    }

    /// Query emitted Events that are defined in the given Move Module.
    pub async fn query_events_by_module(
        &self,
        package: ObjectID,
        module: Identifier,
        // cursor is exclusive
        cursor: Option<EventID>,
    ) -> BridgeResult<Page<SuiEvent, EventID>> {
        let filter = EventFilter::MoveEventModule {
            package,
            module: module.clone(),
        };
        let events = self.inner.query_events(filter.clone(), cursor).await?;

        // Safeguard check that all events are emitted from requested package and module
        assert!(
            events
                .data
                .iter()
                .all(|event| event.type_.address.as_ref() == package.as_ref()
                    && event.type_.module == module)
        );
        Ok(events)
    }

    /// Returns BridgeAction from a Sui Transaction with transaction hash
    /// and the event index. If event is declared in an unrecognized
    /// package, return error.
    pub async fn get_bridge_action_by_tx_digest_and_event_idx_maybe(
        &self,
        tx_digest: &TransactionDigest,
        event_idx: u16,
    ) -> BridgeResult<BridgeAction> {
        let events = self.inner.get_events_by_tx_digest(*tx_digest).await?;
        let event = events
            .events
            .get(event_idx as usize)
            .ok_or(BridgeError::NoBridgeEventsInTxPosition)?;
        if event.type_.address.as_ref() != BRIDGE_PACKAGE_ID.as_ref() {
            return Err(BridgeError::BridgeEventInUnrecognizedSuiPackage);
        }
        let bridge_event = SuiBridgeEvent::try_from_sui_event(event)?
            .ok_or(BridgeError::NoBridgeEventsInTxPosition)?;

        bridge_event
            .try_into_bridge_action(*tx_digest, event_idx)
            .ok_or(BridgeError::BridgeEventNotActionable)
    }

    pub async fn get_bridge_summary(&self) -> BridgeResult<BridgeSummary> {
        self.inner.get_bridge_summary().await
    }

    pub async fn is_bridge_paused(&self) -> BridgeResult<bool> {
        self.get_bridge_summary()
            .await
            .map(|summary| summary.is_frozen)
    }

    pub async fn get_treasury_summary(&self) -> BridgeResult<BridgeTreasurySummary> {
        Ok(self.get_bridge_summary().await?.treasury)
    }

    pub async fn get_token_id_map(&self) -> BridgeResult<HashMap<u8, TypeTag>> {
        self.get_bridge_summary()
            .await?
            .treasury
            .id_token_type_map
            .into_iter()
            .map(|(id, name)| {
                parse_sui_type_tag(&format!("0x{name}"))
                    .map(|name| (id, name))
                    .map_err(|e| {
                        BridgeError::InternalError(format!(
                            "Failed to retrieve token id mapping: {e}, type name: {name}"
                        ))
                    })
            })
            .collect()
    }

    pub async fn get_notional_values(&self) -> BridgeResult<HashMap<u8, u64>> {
        let bridge_summary = self.get_bridge_summary().await?;
        bridge_summary
            .treasury
            .id_token_type_map
            .iter()
            .map(|(id, type_name)| {
                bridge_summary
                    .treasury
                    .supported_tokens
                    .iter()
                    .find_map(|(tn, metadata)| {
                        if type_name == tn {
                            Some((*id, metadata.notional_value))
                        } else {
                            None
                        }
                    })
                    .ok_or(BridgeError::InternalError(
                        "Error encountered when retrieving token notional values.".into(),
                    ))
            })
            .collect()
    }

    pub async fn get_bridge_committee(&self) -> BridgeResult<BridgeCommittee> {
        let bridge_summary = self.inner.get_bridge_summary().await?;
        let move_type_bridge_committee = bridge_summary.committee;

        let mut authorities = vec![];
        // TODO: move this to MoveTypeBridgeCommittee
        for (_, member) in move_type_bridge_committee.members {
            let MoveTypeCommitteeMember {
                sui_address,
                bridge_pubkey_bytes,
                voting_power,
                http_rest_url,
                blocklisted,
            } = member;
            let pubkey = BridgeAuthorityPublicKey::from_bytes(&bridge_pubkey_bytes)?;
            let base_url = from_utf8(&http_rest_url).unwrap_or_else(|_e| {
                warn!(
                    "Bridge authority address: {}, pubkey: {:?} has invalid http url: {:?}",
                    sui_address, bridge_pubkey_bytes, http_rest_url
                );
                ""
            });
            authorities.push(BridgeAuthority {
                sui_address,
                pubkey,
                voting_power,
                base_url: base_url.into(),
                is_blocklisted: blocklisted,
            });
        }
        BridgeCommittee::new(authorities)
    }

    pub async fn get_chain_identifier(&self) -> BridgeResult<String> {
        self.inner.get_chain_identifier().await
    }

    pub async fn get_reference_gas_price_until_success(&self) -> u64 {
        loop {
            let Ok(Ok(rgp)) = retry_with_max_elapsed_time!(
                self.inner.get_reference_gas_price(),
                Duration::from_secs(30)
            ) else {
                self.bridge_metrics
                    .sui_rpc_errors
                    .with_label_values(&["get_reference_gas_price"])
                    .inc();
                error!("Failed to get reference gas price");
                continue;
            };
            return rgp;
        }
    }

    pub async fn get_latest_checkpoint_sequence_number(&self) -> BridgeResult<u64> {
        self.inner.get_latest_checkpoint_sequence_number().await
    }

    pub async fn execute_transaction_block_with_effects(
        &self,
        tx: sui_types::transaction::Transaction,
    ) -> BridgeResult<SuiTransactionBlockResponse> {
        self.inner.execute_transaction_block_with_effects(tx).await
    }

    // TODO: this function is very slow (seconds) in tests, we need to optimize it
    pub async fn get_token_transfer_action_onchain_status_until_success(
        &self,
        source_chain_id: u8,
        seq_number: u64,
    ) -> BridgeActionStatus {
        loop {
            let bridge_object_arg = self.get_mutable_bridge_object_arg_must_succeed().await;
            let Ok(Ok(status)) = retry_with_max_elapsed_time!(
                self.inner.get_token_transfer_action_onchain_status(
                    bridge_object_arg,
                    source_chain_id,
                    seq_number
                ),
                Duration::from_secs(30)
            ) else {
                self.bridge_metrics
                    .sui_rpc_errors
                    .with_label_values(&["get_token_transfer_action_onchain_status"])
                    .inc();
                error!(
                    source_chain_id,
                    seq_number, "Failed to get token transfer action onchain status"
                );
                continue;
            };
            return status;
        }
    }

    pub async fn get_token_transfer_action_onchain_signatures_until_success(
        &self,
        source_chain_id: u8,
        seq_number: u64,
    ) -> Option<Vec<Vec<u8>>> {
        loop {
            let bridge_object_arg = self.get_mutable_bridge_object_arg_must_succeed().await;
            let Ok(Ok(sigs)) = retry_with_max_elapsed_time!(
                self.inner.get_token_transfer_action_onchain_signatures(
                    bridge_object_arg,
                    source_chain_id,
                    seq_number
                ),
                Duration::from_secs(30)
            ) else {
                self.bridge_metrics
                    .sui_rpc_errors
                    .with_label_values(&["get_token_transfer_action_onchain_signatures"])
                    .inc();
                error!(
                    source_chain_id,
                    seq_number, "Failed to get token transfer action onchain signatures"
                );
                continue;
            };
            return sigs;
        }
    }

    pub async fn get_parsed_token_transfer_message(
        &self,
        source_chain_id: u8,
        seq_number: u64,
    ) -> BridgeResult<Option<ParsedTokenTransferMessage>> {
        let bridge_object_arg = self.get_mutable_bridge_object_arg_must_succeed().await;
        let message = self
            .inner
            .get_parsed_token_transfer_message(bridge_object_arg, source_chain_id, seq_number)
            .await?;
        Ok(match message {
            Some(payload) => Some(ParsedTokenTransferMessage::try_from(payload)?),
            None => None,
        })
    }

    pub async fn get_bridge_record(
        &self,
        source_chain_id: u8,
        seq_number: u64,
    ) -> Result<Option<MoveTypeBridgeRecord>, BridgeError> {
        self.inner
            .get_bridge_record(source_chain_id, seq_number)
            .await
    }

    pub async fn get_gas_data_panic_if_not_gas(
        &self,
        gas_object_id: ObjectID,
    ) -> (GasCoin, ObjectRef, Owner) {
        self.inner
            .get_gas_data_panic_if_not_gas(gas_object_id)
            .await
    }
}

/// Use a trait to abstract over the SuiSDKClient and SuiMockClient for testing.
#[async_trait]
pub trait SuiClientInner: Send + Sync {
    async fn query_events(
        &self,
        query: EventFilter,
        cursor: Option<EventID>,
    ) -> Result<EventPage, BridgeError>;

    async fn get_events_by_tx_digest(
        &self,
        tx_digest: TransactionDigest,
    ) -> Result<SuiEvents, BridgeError>;

    async fn get_chain_identifier(&self) -> Result<String, BridgeError>;

    async fn get_reference_gas_price(&self) -> Result<u64, BridgeError>;

    async fn get_latest_checkpoint_sequence_number(&self) -> Result<u64, BridgeError>;

    async fn get_mutable_bridge_object_arg(&self) -> Result<ObjectArg, BridgeError>;

    async fn get_bridge_summary(&self) -> Result<BridgeSummary, BridgeError>;

    async fn execute_transaction_block_with_effects(
        &self,
        tx: Transaction,
    ) -> Result<SuiTransactionBlockResponse, BridgeError>;

    async fn get_token_transfer_action_onchain_status(
        &self,
        bridge_object_arg: ObjectArg,
        source_chain_id: u8,
        seq_number: u64,
    ) -> Result<BridgeActionStatus, BridgeError>;

    async fn get_token_transfer_action_onchain_signatures(
        &self,
        bridge_object_arg: ObjectArg,
        source_chain_id: u8,
        seq_number: u64,
    ) -> Result<Option<Vec<Vec<u8>>>, BridgeError>;

    async fn get_parsed_token_transfer_message(
        &self,
        bridge_object_arg: ObjectArg,
        source_chain_id: u8,
        seq_number: u64,
    ) -> Result<Option<MoveTypeParsedTokenTransferMessage>, BridgeError>;

    async fn get_bridge_record(
        &self,
        source_chain_id: u8,
        seq_number: u64,
    ) -> Result<Option<MoveTypeBridgeRecord>, BridgeError>;

    async fn get_gas_data_panic_if_not_gas(
        &self,
        gas_object_id: ObjectID,
    ) -> (GasCoin, ObjectRef, Owner);
}

#[async_trait]
impl SuiClientInner for SuiSdkClient {
    async fn query_events(
        &self,
        query: EventFilter,
        cursor: Option<EventID>,
    ) -> Result<EventPage, BridgeError> {
        self.event_api()
            .query_events(query, cursor, None, false)
            .await
            .map_err(Into::into)
    }

    async fn get_events_by_tx_digest(
        &self,
        _tx_digest: TransactionDigest,
    ) -> Result<SuiEvents, BridgeError> {
        unimplemented!("use gRPC implementation")
    }

    async fn get_chain_identifier(&self) -> Result<String, BridgeError> {
        unimplemented!("use gRPC implementation")
    }

    async fn get_reference_gas_price(&self) -> Result<u64, BridgeError> {
        unimplemented!("use gRPC implementation")
    }

    async fn get_latest_checkpoint_sequence_number(&self) -> Result<u64, BridgeError> {
        unimplemented!("use gRPC implementation")
    }

    async fn get_mutable_bridge_object_arg(&self) -> Result<ObjectArg, BridgeError> {
        unimplemented!("use gRPC implementation")
    }

    async fn get_bridge_summary(&self) -> Result<BridgeSummary, BridgeError> {
        unimplemented!("use gRPC implementation")
    }

    async fn get_token_transfer_action_onchain_status(
        &self,
        _bridge_object_arg: ObjectArg,
        _source_chain_id: u8,
        _seq_number: u64,
    ) -> Result<BridgeActionStatus, BridgeError> {
        unimplemented!("use gRPC implementation")
    }

    async fn get_token_transfer_action_onchain_signatures(
        &self,
        _bridge_object_arg: ObjectArg,
        _source_chain_id: u8,
        _seq_number: u64,
    ) -> Result<Option<Vec<Vec<u8>>>, BridgeError> {
        unimplemented!("use gRPC implementation")
    }

    async fn execute_transaction_block_with_effects(
        &self,
        tx: Transaction,
    ) -> Result<SuiTransactionBlockResponse, BridgeError> {
        match self.quorum_driver_api().execute_transaction_block(
            tx,
            SuiTransactionBlockResponseOptions::new().with_effects().with_events(),
            Some(sui_types::quorum_driver_types::ExecuteTransactionRequestType::WaitForEffectsCert),
        ).await {
            Ok(response) => Ok(response),
            Err(e) => return Err(BridgeError::SuiTxFailureGeneric(e.to_string())),
        }
    }

    async fn get_parsed_token_transfer_message(
        &self,
        _bridge_object_arg: ObjectArg,
        _source_chain_id: u8,
        _seq_number: u64,
    ) -> Result<Option<MoveTypeParsedTokenTransferMessage>, BridgeError> {
        unimplemented!("use gRPC implementation")
    }

    async fn get_bridge_record(
        &self,
        _source_chain_id: u8,
        _seq_number: u64,
    ) -> Result<Option<MoveTypeBridgeRecord>, BridgeError> {
        unimplemented!("use gRPC implementation")
    }

    async fn get_gas_data_panic_if_not_gas(
        &self,
        gas_object_id: ObjectID,
    ) -> (GasCoin, ObjectRef, Owner) {
        loop {
            match self
                .read_api()
                .get_object_with_options(
                    gas_object_id,
                    SuiObjectDataOptions::default().with_owner().with_content(),
                )
                .await
                .map(|resp| resp.data)
            {
                Ok(Some(gas_obj)) => {
                    let owner = gas_obj.owner.clone().expect("Owner is requested");
                    let gas_coin = GasCoin::try_from(&gas_obj)
                        .unwrap_or_else(|err| panic!("{} is not a gas coin: {err}", gas_object_id));
                    return (gas_coin, gas_obj.object_ref(), owner);
                }
                other => {
                    warn!("Can't get gas object: {:?}: {:?}", gas_object_id, other);
                    tokio::time::sleep(Duration::from_secs(5)).await;
                }
            }
        }
    }
}

#[async_trait]
impl SuiClientInner for sui_rpc::Client {
    async fn query_events(
        &self,
        _query: EventFilter,
        _cursor: Option<EventID>,
    ) -> Result<EventPage, BridgeError> {
        //TODO we'll need to reimplement the sui_syncer to iterate though records instead of
        //querying events using this api
        unimplemented!("query_events not supported in gRPC");
    }

    async fn get_events_by_tx_digest(
        &self,
        tx_digest: TransactionDigest,
    ) -> Result<SuiEvents, BridgeError> {
        let mut client = self.clone();
        let resp = client
            .ledger_client()
            .get_transaction(
                GetTransactionRequest::new(&(tx_digest.into())).with_read_mask(
                    FieldMask::from_paths([
                        ExecutedTransaction::path_builder().digest(),
                        ExecutedTransaction::path_builder().events().finish(),
                        ExecutedTransaction::path_builder().checkpoint(),
                        ExecutedTransaction::path_builder().timestamp(),
                    ]),
                ),
            )
            .await?
            .into_inner();
        let resp = resp.transaction();

        Ok(SuiEvents {
            transaction_digest: tx_digest,
            checkpoint: resp.checkpoint_opt(),
            timestamp_ms: resp
                .timestamp_opt()
                .map(|timestamp| sui_rpc::proto::proto_to_timestamp_ms(*timestamp))
                .transpose()?,
            events: resp
                .events()
                .events()
                .iter()
                .enumerate()
                .map(|(idx, event)| {
                    Ok(SuiEvent {
                        id: EventID {
                            tx_digest,
                            event_seq: idx as u64,
                        },
                        package_id: event.package_id().parse()?,
                        transaction_module: Identifier::new(event.module())?,
                        sender: event.sender().parse()?,
                        type_: event.event_type().parse()?,
                        parsed_json: Default::default(),
                        bcs: BcsEvent::Base64 {
                            bcs: event.contents().value().into(),
                        },
                        timestamp_ms: None,
                    })
                })
                .collect::<Result<_, BridgeError>>()?,
        })
    }

    async fn get_chain_identifier(&self) -> Result<String, BridgeError> {
        Ok(self
            .clone()
            .ledger_client()
            .get_service_info(GetServiceInfoRequest::default())
            .await?
            .into_inner()
            .chain_id()
            .into())
    }

    async fn get_reference_gas_price(&self) -> Result<u64, BridgeError> {
        let mut client = self.clone();
        sui_rpc::Client::get_reference_gas_price(&mut client)
            .await
            .map_err(Into::into)
    }

    async fn get_latest_checkpoint_sequence_number(&self) -> Result<u64, BridgeError> {
        let mut client = self.clone();
        let resp =
            client
                .ledger_client()
                .get_checkpoint(GetCheckpointRequest::latest().with_read_mask(
                    FieldMask::from_paths([Checkpoint::path_builder().sequence_number()]),
                ))
                .await?
                .into_inner();
        Ok(resp.checkpoint().sequence_number())
    }

    async fn get_mutable_bridge_object_arg(&self) -> Result<ObjectArg, BridgeError> {
        let owner = self
            .clone()
            .ledger_client()
            .get_object(
                GetObjectRequest::new(&(SUI_BRIDGE_OBJECT_ID.into())).with_read_mask(
                    FieldMask::from_paths([Object::path_builder().owner().finish()]),
                ),
            )
            .await?
            .into_inner()
            .object()
            .owner()
            .to_owned();
        Ok(ObjectArg::SharedObject {
            id: SUI_BRIDGE_OBJECT_ID,
            initial_shared_version: SequenceNumber::from_u64(owner.version()),
            mutability: SharedObjectMutability::Mutable,
        })
    }

    async fn get_bridge_summary(&self) -> Result<BridgeSummary, BridgeError> {
        static BRIDGE_VERSION_ID: tokio::sync::OnceCell<Address> =
            tokio::sync::OnceCell::const_new();

        let bridge_version_id = BRIDGE_VERSION_ID
            .get_or_try_init::<BridgeError, _, _>(|| async {
                let bridge_wrapper_bcs = self
                    .clone()
                    .ledger_client()
                    .get_object(
                        GetObjectRequest::new(&(SUI_BRIDGE_OBJECT_ID.into())).with_read_mask(
                            FieldMask::from_paths([Object::path_builder().contents().finish()]),
                        ),
                    )
                    .await?
                    .into_inner()
                    .object()
                    .contents()
                    .to_owned();

                let bridge_wrapper: BridgeWrapper = bcs::from_bytes(bridge_wrapper_bcs.value())?;

                Ok(bridge_wrapper.version.id.id.bytes.into())
            })
            .await?;

        let bridge_inner_id = bridge_version_id
            .derive_dynamic_child_id(&sui_sdk_types::TypeTag::U64, &bcs::to_bytes(&1u64).unwrap());

        let field_bcs = self
            .clone()
            .ledger_client()
            .get_object(GetObjectRequest::new(&bridge_inner_id).with_read_mask(
                FieldMask::from_paths([Object::path_builder().contents().finish()]),
            ))
            .await?
            .into_inner()
            .object()
            .contents()
            .to_owned();

        let field: sui_types::dynamic_field::Field<u64, sui_types::bridge::BridgeInnerV1> =
            bcs::from_bytes(field_bcs.value())?;
        let summary = field.value.try_into_bridge_summary()?;
        Ok(summary)
    }

    async fn get_token_transfer_action_onchain_status(
        &self,
        _bridge_object_arg: ObjectArg,
        source_chain_id: u8,
        seq_number: u64,
    ) -> Result<BridgeActionStatus, BridgeError> {
        let record = self.get_bridge_record(source_chain_id, seq_number).await?;
        let Some(record) = record else {
            return Ok(BridgeActionStatus::NotFound);
        };

        if record.claimed {
            Ok(BridgeActionStatus::Claimed)
        } else if record.verified_signatures.is_some() {
            Ok(BridgeActionStatus::Approved)
        } else {
            Ok(BridgeActionStatus::Pending)
        }
    }

    async fn get_token_transfer_action_onchain_signatures(
        &self,
        _bridge_object_arg: ObjectArg,
        source_chain_id: u8,
        seq_number: u64,
    ) -> Result<Option<Vec<Vec<u8>>>, BridgeError> {
        let record = self.get_bridge_record(source_chain_id, seq_number).await?;
        Ok(record.and_then(|record| record.verified_signatures))
    }

    async fn execute_transaction_block_with_effects(
        &self,
        _tx: Transaction,
    ) -> Result<SuiTransactionBlockResponse, BridgeError> {
        todo!()
    }

    async fn get_parsed_token_transfer_message(
        &self,
        _bridge_object_arg: ObjectArg,
        source_chain_id: u8,
        seq_number: u64,
    ) -> Result<Option<MoveTypeParsedTokenTransferMessage>, BridgeError> {
        let record = self.get_bridge_record(source_chain_id, seq_number).await?;

        let Some(record) = record else {
            return Ok(None);
        };
        let MoveTypeBridgeMessage {
            message_type: _,
            message_version,
            seq_num,
            source_chain,
            payload,
        } = record.message;

        let mut parsed_payload: MoveTypeTokenTransferPayload = bcs::from_bytes(&payload)?;

        // we deser'd le bytes but this needs to be interpreted as be bytes
        parsed_payload.amount = u64::from_be_bytes(parsed_payload.amount.to_le_bytes());

        Ok(Some(MoveTypeParsedTokenTransferMessage {
            message_version,
            seq_num,
            source_chain,
            payload,
            parsed_payload,
        }))
    }

    async fn get_bridge_record(
        &self,
        source_chain_id: u8,
        seq_number: u64,
    ) -> Result<Option<MoveTypeBridgeRecord>, BridgeError> {
        static BRIDGE_RECORDS_ID: tokio::sync::OnceCell<Address> =
            tokio::sync::OnceCell::const_new();

        let records_id = BRIDGE_RECORDS_ID
            .get_or_try_init(|| async {
                self.get_bridge_summary()
                    .await
                    .map(|summary| summary.bridge_records_id.into())
            })
            .await?;

        let record_id = {
            let key = MoveTypeBridgeMessageKey {
                source_chain: source_chain_id,
                message_type: crate::types::BridgeActionType::TokenTransfer as u8,
                bridge_seq_num: seq_number,
            };
            let key_bytes = bcs::to_bytes(&key)?;
            let key_type = sui_sdk_types::StructTag {
                address: Address::from(BRIDGE_PACKAGE_ID),
                module: sui_sdk_types::Identifier::from_static("message"),
                name: sui_sdk_types::Identifier::from_static("BridgeMessageKey"),
                type_params: vec![],
            };

            records_id.derive_dynamic_child_id(&(key_type.into()), &key_bytes)
        };

        let response =
            match self
                .clone()
                .ledger_client()
                .get_object(GetObjectRequest::new(&record_id).with_read_mask(
                    FieldMask::from_paths([Object::path_builder().contents().finish()]),
                ))
                .await
            {
                Ok(response) => response,
                Err(status) => {
                    if status.code() == tonic::Code::NotFound {
                        return Ok(None);
                    } else {
                        return Err(status.into());
                    }
                }
            };

        let field_bcs = response.into_inner().object().contents().to_owned();

        let field: sui_types::dynamic_field::Field<
            MoveTypeBridgeMessageKey,
            LinkedTableNode<MoveTypeBridgeMessageKey, MoveTypeBridgeRecord>,
        > = bcs::from_bytes(field_bcs.value())?;

        Ok(Some(field.value.value))
    }

    async fn get_gas_data_panic_if_not_gas(
        &self,
        _gas_object_id: ObjectID,
    ) -> (GasCoin, ObjectRef, Owner) {
        todo!()
    }
}

#[async_trait]
impl SuiClientInner for SuiClientInternal {
    async fn query_events(
        &self,
        query: EventFilter,
        cursor: Option<EventID>,
    ) -> Result<EventPage, BridgeError> {
        self.jsonrpc_client.query_events(query, cursor).await
    }

    async fn get_events_by_tx_digest(
        &self,
        tx_digest: TransactionDigest,
    ) -> Result<SuiEvents, BridgeError> {
        self.grpc_client.get_events_by_tx_digest(tx_digest).await
    }

    async fn get_chain_identifier(&self) -> Result<String, BridgeError> {
        self.grpc_client.get_chain_identifier().await
    }

    async fn get_reference_gas_price(&self) -> Result<u64, BridgeError> {
        self.grpc_client.get_reference_gas_price().await
    }

    async fn get_latest_checkpoint_sequence_number(&self) -> Result<u64, BridgeError> {
        self.grpc_client
            .get_latest_checkpoint_sequence_number()
            .await
    }

    async fn get_mutable_bridge_object_arg(&self) -> Result<ObjectArg, BridgeError> {
        self.grpc_client.get_mutable_bridge_object_arg().await
    }

    async fn get_bridge_summary(&self) -> Result<BridgeSummary, BridgeError> {
        self.grpc_client.get_bridge_summary().await
    }

    async fn get_token_transfer_action_onchain_status(
        &self,
        bridge_object_arg: ObjectArg,
        source_chain_id: u8,
        seq_number: u64,
    ) -> Result<BridgeActionStatus, BridgeError> {
        self.grpc_client
            .get_token_transfer_action_onchain_status(
                bridge_object_arg,
                source_chain_id,
                seq_number,
            )
            .await
    }

    async fn get_token_transfer_action_onchain_signatures(
        &self,
        bridge_object_arg: ObjectArg,
        source_chain_id: u8,
        seq_number: u64,
    ) -> Result<Option<Vec<Vec<u8>>>, BridgeError> {
        self.grpc_client
            .get_token_transfer_action_onchain_signatures(
                bridge_object_arg,
                source_chain_id,
                seq_number,
            )
            .await
    }

    async fn execute_transaction_block_with_effects(
        &self,
        tx: Transaction,
    ) -> Result<SuiTransactionBlockResponse, BridgeError> {
        self.jsonrpc_client
            .execute_transaction_block_with_effects(tx)
            .await
    }

    async fn get_parsed_token_transfer_message(
        &self,
        bridge_object_arg: ObjectArg,
        source_chain_id: u8,
        seq_number: u64,
    ) -> Result<Option<MoveTypeParsedTokenTransferMessage>, BridgeError> {
        self.grpc_client
            .get_parsed_token_transfer_message(bridge_object_arg, source_chain_id, seq_number)
            .await
    }

    async fn get_bridge_record(
        &self,
        source_chain_id: u8,
        seq_number: u64,
    ) -> Result<Option<MoveTypeBridgeRecord>, BridgeError> {
        self.grpc_client
            .get_bridge_record(source_chain_id, seq_number)
            .await
    }

    async fn get_gas_data_panic_if_not_gas(
        &self,
        gas_object_id: ObjectID,
    ) -> (GasCoin, ObjectRef, Owner) {
        self.jsonrpc_client
            .get_gas_data_panic_if_not_gas(gas_object_id)
            .await
    }
}

#[cfg(test)]
mod tests {
    use crate::crypto::BridgeAuthorityKeyPair;
    use crate::e2e_tests::test_utils::TestClusterWrapperBuilder;
    use crate::{
        events::{EmittedSuiToEthTokenBridgeV1, MoveTokenDepositedEvent},
        sui_mock_client::SuiMockClient,
        test_utils::{
            approve_action_with_validator_secrets, bridge_token, get_test_eth_to_sui_bridge_action,
            get_test_sui_to_eth_bridge_action,
        },
        types::SuiToEthBridgeAction,
    };
    use ethers::types::Address as EthAddress;
    use move_core_types::account_address::AccountAddress;
    use serde::{Deserialize, Serialize};
    use std::str::FromStr;
    use sui_json_rpc_types::BcsEvent;
    use sui_types::base_types::SuiAddress;
    use sui_types::bridge::{BridgeChainId, TOKEN_ID_SUI, TOKEN_ID_USDC};
    use sui_types::crypto::get_key_pair;

    use super::*;
    use crate::events::{SuiToEthTokenBridgeV1, init_all_struct_tags};

    #[tokio::test]
    async fn get_bridge_action_by_tx_digest_and_event_idx_maybe() {
        // Note: for random events generated in this test, we only care about
        // tx_digest and event_seq, so it's ok that package and module does
        // not match the query parameters.
        telemetry_subscribers::init_for_testing();
        let mock_client = SuiMockClient::default();
        let sui_client = SuiClient::new_for_testing(mock_client.clone());
        let tx_digest = TransactionDigest::random();

        // Ensure all struct tags are inited
        init_all_struct_tags();

        let sanitized_event_1 = EmittedSuiToEthTokenBridgeV1 {
            nonce: 1,
            sui_chain_id: BridgeChainId::SuiTestnet,
            sui_address: SuiAddress::random_for_testing_only(),
            eth_chain_id: BridgeChainId::EthSepolia,
            eth_address: EthAddress::random(),
            token_id: TOKEN_ID_SUI,
            amount_sui_adjusted: 100,
        };
        let emitted_event_1 = MoveTokenDepositedEvent {
            seq_num: sanitized_event_1.nonce,
            source_chain: sanitized_event_1.sui_chain_id as u8,
            sender_address: sanitized_event_1.sui_address.to_vec(),
            target_chain: sanitized_event_1.eth_chain_id as u8,
            target_address: sanitized_event_1.eth_address.as_bytes().to_vec(),
            token_type: sanitized_event_1.token_id,
            amount_sui_adjusted: sanitized_event_1.amount_sui_adjusted,
        };

        let mut sui_event_1 = SuiEvent::random_for_testing();
        sui_event_1.type_ = SuiToEthTokenBridgeV1.get().unwrap().clone();
        sui_event_1.bcs = BcsEvent::new(bcs::to_bytes(&emitted_event_1).unwrap());

        #[derive(Serialize, Deserialize)]
        struct RandomStruct {}

        let event_2: RandomStruct = RandomStruct {};
        // undeclared struct tag
        let mut sui_event_2 = SuiEvent::random_for_testing();
        sui_event_2.type_ = SuiToEthTokenBridgeV1.get().unwrap().clone();
        sui_event_2.type_.module = Identifier::from_str("unrecognized_module").unwrap();
        sui_event_2.bcs = BcsEvent::new(bcs::to_bytes(&event_2).unwrap());

        // Event 3 is defined in non-bridge package
        let mut sui_event_3 = sui_event_1.clone();
        sui_event_3.type_.address = AccountAddress::random();

        mock_client.add_events_by_tx_digest(
            tx_digest,
            vec![
                sui_event_1.clone(),
                sui_event_2.clone(),
                sui_event_1.clone(),
                sui_event_3.clone(),
            ],
        );
        let expected_action_1 = BridgeAction::SuiToEthBridgeAction(SuiToEthBridgeAction {
            sui_tx_digest: tx_digest,
            sui_tx_event_index: 0,
            sui_bridge_event: sanitized_event_1.clone(),
        });
        assert_eq!(
            sui_client
                .get_bridge_action_by_tx_digest_and_event_idx_maybe(&tx_digest, 0)
                .await
                .unwrap(),
            expected_action_1,
        );
        let expected_action_2 = BridgeAction::SuiToEthBridgeAction(SuiToEthBridgeAction {
            sui_tx_digest: tx_digest,
            sui_tx_event_index: 2,
            sui_bridge_event: sanitized_event_1.clone(),
        });
        assert_eq!(
            sui_client
                .get_bridge_action_by_tx_digest_and_event_idx_maybe(&tx_digest, 2)
                .await
                .unwrap(),
            expected_action_2,
        );
        assert!(matches!(
            sui_client
                .get_bridge_action_by_tx_digest_and_event_idx_maybe(&tx_digest, 1)
                .await
                .unwrap_err(),
            BridgeError::NoBridgeEventsInTxPosition
        ),);
        assert!(matches!(
            sui_client
                .get_bridge_action_by_tx_digest_and_event_idx_maybe(&tx_digest, 3)
                .await
                .unwrap_err(),
            BridgeError::BridgeEventInUnrecognizedSuiPackage
        ),);
        assert!(matches!(
            sui_client
                .get_bridge_action_by_tx_digest_and_event_idx_maybe(&tx_digest, 4)
                .await
                .unwrap_err(),
            BridgeError::NoBridgeEventsInTxPosition
        ),);

        // if the StructTag matches with unparsable bcs, it returns an error
        sui_event_2.type_ = SuiToEthTokenBridgeV1.get().unwrap().clone();
        mock_client.add_events_by_tx_digest(tx_digest, vec![sui_event_2]);
        sui_client
            .get_bridge_action_by_tx_digest_and_event_idx_maybe(&tx_digest, 2)
            .await
            .unwrap_err();
    }

    // Test get_action_onchain_status.
    // Use validator secrets to bridge USDC from Ethereum initially.
    // TODO: we need an e2e test for this with published solidity contract and committee with BridgeNodes
    #[tokio::test(flavor = "multi_thread", worker_threads = 8)]
    async fn test_get_action_onchain_status_for_sui_to_eth_transfer() {
        telemetry_subscribers::init_for_testing();
        let mut bridge_keys = vec![];
        for _ in 0..=3 {
            let (_, kp): (_, BridgeAuthorityKeyPair) = get_key_pair();
            bridge_keys.push(kp);
        }
        let mut test_cluster = TestClusterWrapperBuilder::new()
            .with_bridge_authority_keys(bridge_keys)
            .with_deploy_tokens(true)
            .build()
            .await;

        let bridge_metrics = Arc::new(BridgeMetrics::new_for_testing());
        let sui_client =
            SuiClient::new(&test_cluster.inner.fullnode_handle.rpc_url, bridge_metrics)
                .await
                .unwrap();
        let bridge_authority_keys = test_cluster.authority_keys_clone();

        // Wait until committee is set up
        test_cluster
            .trigger_reconfiguration_if_not_yet_and_assert_bridge_committee_initialized()
            .await;
        let context = &mut test_cluster.inner.wallet;
        let sender = context.active_address().unwrap();
        let usdc_amount = 5000000;
        let bridge_object_arg = sui_client
            .get_mutable_bridge_object_arg_must_succeed()
            .await;
        let id_token_map = sui_client.get_token_id_map().await.unwrap();

        // 1. Create a Eth -> Sui Transfer (recipient is sender address), approve with validator secrets and assert its status to be Claimed
        let action = get_test_eth_to_sui_bridge_action(None, Some(usdc_amount), Some(sender), None);
        let usdc_object_ref = approve_action_with_validator_secrets(
            context,
            bridge_object_arg,
            action.clone(),
            &bridge_authority_keys,
            Some(sender),
            &id_token_map,
        )
        .await
        .unwrap();

        let status = sui_client
            .inner
            .get_token_transfer_action_onchain_status(
                bridge_object_arg,
                action.chain_id() as u8,
                action.seq_number(),
            )
            .await
            .unwrap();
        assert_eq!(status, BridgeActionStatus::Claimed);

        // 2. Create a Sui -> Eth Transfer, approve with validator secrets and assert its status to be Approved
        // We need to actually send tokens to bridge to initialize the record.
        let eth_recv_address = EthAddress::random();
        let bridge_event = bridge_token(
            context,
            eth_recv_address,
            usdc_object_ref,
            id_token_map.get(&TOKEN_ID_USDC).unwrap().clone(),
            bridge_object_arg,
        )
        .await;
        assert_eq!(bridge_event.nonce, 0);
        assert_eq!(bridge_event.sui_chain_id, BridgeChainId::SuiCustom);
        assert_eq!(bridge_event.eth_chain_id, BridgeChainId::EthCustom);
        assert_eq!(bridge_event.eth_address, eth_recv_address);
        assert_eq!(bridge_event.sui_address, sender);
        assert_eq!(bridge_event.token_id, TOKEN_ID_USDC);
        assert_eq!(bridge_event.amount_sui_adjusted, usdc_amount);

        let action = get_test_sui_to_eth_bridge_action(
            None,
            None,
            Some(bridge_event.nonce),
            Some(bridge_event.amount_sui_adjusted),
            Some(bridge_event.sui_address),
            Some(bridge_event.eth_address),
            Some(TOKEN_ID_USDC),
        );
        let status = sui_client
            .inner
            .get_token_transfer_action_onchain_status(
                bridge_object_arg,
                action.chain_id() as u8,
                action.seq_number(),
            )
            .await
            .unwrap();
        // At this point, the record is created and the status is Pending
        assert_eq!(status, BridgeActionStatus::Pending);

        // Approve it and assert its status to be Approved
        approve_action_with_validator_secrets(
            context,
            bridge_object_arg,
            action.clone(),
            &bridge_authority_keys,
            None,
            &id_token_map,
        )
        .await;

        let status = sui_client
            .inner
            .get_token_transfer_action_onchain_status(
                bridge_object_arg,
                action.chain_id() as u8,
                action.seq_number(),
            )
            .await
            .unwrap();
        assert_eq!(status, BridgeActionStatus::Approved);

        // 3. Create a random action and assert its status as NotFound
        let action =
            get_test_sui_to_eth_bridge_action(None, None, Some(100), None, None, None, None);
        let status = sui_client
            .inner
            .get_token_transfer_action_onchain_status(
                bridge_object_arg,
                action.chain_id() as u8,
                action.seq_number(),
            )
            .await
            .unwrap();
        assert_eq!(status, BridgeActionStatus::NotFound);
    }
}
