// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use std::sync::Arc;

use prost_types::FieldMask;
use sui_rpc::field::{FieldMaskTree, FieldMaskUtil};
use sui_rpc::merge::Merge;
use sui_rpc::proto::sui::rpc::v2::{
    ledger_service_server::LedgerService, BatchGetObjectsRequest, BatchGetObjectsResponse,
    BatchGetTransactionsRequest, BatchGetTransactionsResponse, ExecutedTransaction,
    GetCheckpointRequest, GetCheckpointResponse, GetEpochRequest, GetEpochResponse,
    GetObjectRequest, GetObjectResponse, GetObjectResult, GetServiceInfoRequest,
    GetServiceInfoResponse, GetTransactionRequest, GetTransactionResponse, GetTransactionResult,
    Object, Transaction, TransactionEffects, TransactionEvents, UserSignature,
};
use sui_rpc_api::grpc::v2::ledger_service::protocol_config_to_proto;
use sui_rpc_api::grpc::v2::ledger_service::validate_get_object_requests;
use sui_rpc_api::proto::google::rpc::bad_request::FieldViolation;
use sui_rpc_api::{
    CheckpointNotFoundError, ErrorReason, ObjectNotFoundError, RpcError, TransactionNotFoundError,
};
use sui_sdk_types::Digest;
use sui_types::base_types::ObjectID;
use sui_types::digests::{ChainIdentifier, CheckpointDigest};
use tokio::sync::RwLock;

use crate::store::ForkingStore;
use fastcrypto::encoding::{Base58, Encoding};
use sui_rpc::proto::sui::rpc::v2::get_checkpoint_request::CheckpointId;
use sui_rpc::proto::sui::rpc::v2::{Checkpoint, Epoch, ValidatorCommittee};
use sui_rpc_api::proto::sui::rpc::v2::ValidatorCommitteeMember;
use sui_types::message_envelope::Message;
use sui_types::sui_system_state::epoch_start_sui_system_state::EpochStartSystemStateTrait;
use tracing::info;

const READ_MASK_DEFAULT: &str = "digest";

/// A LedgerService implementation backed by the ForkingStore/Simulacrum.
pub struct ForkingLedgerService {
    simulacrum: Arc<RwLock<simulacrum::Simulacrum<rand::rngs::OsRng, ForkingStore>>>,
    chain_id: ChainIdentifier,
}

impl ForkingLedgerService {
    pub fn new(
        simulacrum: Arc<RwLock<simulacrum::Simulacrum<rand::rngs::OsRng, ForkingStore>>>,
        chain_id: ChainIdentifier,
    ) -> Self {
        Self {
            simulacrum,
            chain_id,
        }
    }
}

#[tonic::async_trait]
impl LedgerService for ForkingLedgerService {
    async fn get_service_info(
        &self,
        _request: tonic::Request<GetServiceInfoRequest>,
    ) -> Result<tonic::Response<GetServiceInfoResponse>, tonic::Status> {
        let sim = self.simulacrum.read().await;
        let store = sim.store();

        let checkpoint = store.get_highest_checkpint();
        let mut message = GetServiceInfoResponse::default();

        message.chain_id = Some(Base58::encode(self.chain_id.as_bytes()));
        message.chain = Some(self.chain_id.chain().as_str().into());

        if let Some(cp) = checkpoint {
            message.epoch = Some(cp.epoch());
            message.checkpoint_height = Some(cp.sequence_number);
            message.timestamp = Some(sui_rpc_api::proto::timestamp_ms_to_proto(cp.timestamp_ms));
        }

        message.lowest_available_checkpoint = Some(0);
        message.lowest_available_checkpoint_objects = Some(0);
        message.server = Some("sui-forking".to_string());

        Ok(tonic::Response::new(message))
    }

    async fn get_object(
        &self,
        request: tonic::Request<GetObjectRequest>,
    ) -> Result<tonic::Response<GetObjectResponse>, tonic::Status> {
        let GetObjectRequest {
            object_id,
            version,
            read_mask,
            ..
        } = request.into_inner();

        println!(
            "Received get_object request for object_id: {:?}, version: {:?}",
            object_id, version
        );

        let (requests, read_mask) =
            validate_get_object_requests(vec![(object_id, version)], read_mask)
                .map_err(|e| tonic::Status::from(e))?;

        let (object_id, version) = requests[0];
        let object = self
            .get_object_impl(object_id.into(), version)
            .await
            .map_err(|e| tonic::Status::from(e))?;

        let mut proto_object = Object::default();
        proto_object.merge(&object, &read_mask);

        Ok(tonic::Response::new(GetObjectResponse::new(proto_object)))
    }

    async fn batch_get_objects(
        &self,
        request: tonic::Request<BatchGetObjectsRequest>,
    ) -> Result<tonic::Response<BatchGetObjectsResponse>, tonic::Status> {
        let BatchGetObjectsRequest {
            requests,
            read_mask,
            ..
        } = request.into_inner();

        println!(
            "Received batch_get_object request for ids {:?}",
            requests.iter().map(|o| o.object_id()).collect::<Vec<_>>()
        );

        let requests: Vec<_> = requests
            .into_iter()
            .map(|req| (req.object_id, req.version))
            .collect();

        let (requests, read_mask) = validate_get_object_requests(requests, read_mask)
            .map_err(|e| tonic::Status::from(e))?;

        let mut results = Vec::with_capacity(requests.len());

        for (object_id, version) in requests {
            let result = match self.get_object_impl(object_id.into(), version).await {
                Ok(object) => {
                    let mut proto_object = Object::default();
                    proto_object.merge(&object, &read_mask);
                    GetObjectResult::new_object(proto_object)
                }
                Err(err) => GetObjectResult::new_error(err.into_status_proto()),
            };
            results.push(result);
        }

        info!(
            "Collected objects results for batch_get_objects {:?}",
            results
                .iter()
                .map(|r| r.object().object_id())
                .collect::<Vec<_>>()
        );

        Ok(tonic::Response::new(BatchGetObjectsResponse::new(results)))
    }

    async fn get_transaction(
        &self,
        request: tonic::Request<GetTransactionRequest>,
    ) -> Result<tonic::Response<GetTransactionResponse>, tonic::Status> {
        let GetTransactionRequest {
            digest, read_mask, ..
        } = request.into_inner();

        let transaction_digest: Digest = digest
            .ok_or_else(|| {
                FieldViolation::new("digest")
                    .with_description("missing digest")
                    .with_reason(ErrorReason::FieldMissing)
            })
            .map_err(|e| tonic::Status::from(RpcError::from(e)))?
            .parse()
            .map_err(|e| {
                let fv = FieldViolation::new("digest")
                    .with_description(format!("invalid digest: {e}"))
                    .with_reason(ErrorReason::FieldInvalid);
                tonic::Status::from(RpcError::from(fv))
            })?;

        let read_mask = {
            let read_mask = read_mask.unwrap_or_else(|| FieldMask::from_str(READ_MASK_DEFAULT));
            read_mask
                .validate::<ExecutedTransaction>()
                .map_err(|path| {
                    let fv = FieldViolation::new("read_mask")
                        .with_description(format!("invalid read_mask path: {path}"))
                        .with_reason(ErrorReason::FieldInvalid);
                    tonic::Status::from(RpcError::from(fv))
                })?;
            FieldMaskTree::from(read_mask)
        };

        let transaction = self
            .get_transaction_impl(transaction_digest.into(), &read_mask)
            .await
            .map_err(|e| tonic::Status::from(e))?;

        Ok(tonic::Response::new(GetTransactionResponse::new(
            transaction,
        )))
    }

    async fn batch_get_transactions(
        &self,
        request: tonic::Request<BatchGetTransactionsRequest>,
    ) -> Result<tonic::Response<BatchGetTransactionsResponse>, tonic::Status> {
        let BatchGetTransactionsRequest {
            digests, read_mask, ..
        } = request.into_inner();

        let read_mask = {
            let read_mask = read_mask.unwrap_or_else(|| FieldMask::from_str(READ_MASK_DEFAULT));
            read_mask
                .validate::<ExecutedTransaction>()
                .map_err(|path| {
                    let fv = FieldViolation::new("read_mask")
                        .with_description(format!("invalid read_mask path: {path}"))
                        .with_reason(ErrorReason::FieldInvalid);
                    tonic::Status::from(RpcError::from(fv))
                })?;
            FieldMaskTree::from(read_mask)
        };

        let mut results = Vec::with_capacity(digests.len());

        for (idx, digest_str) in digests.into_iter().enumerate() {
            let result = match digest_str.parse::<Digest>() {
                Ok(digest) => match self.get_transaction_impl(digest.into(), &read_mask).await {
                    Ok(tx) => GetTransactionResult::new_transaction(tx),
                    Err(err) => GetTransactionResult::new_error(err.into_status_proto()),
                },
                Err(e) => {
                    let fv = FieldViolation::new_at("digests", idx)
                        .with_description(format!("invalid digest: {e}"))
                        .with_reason(ErrorReason::FieldInvalid);
                    GetTransactionResult::new_error(RpcError::from(fv).into_status_proto())
                }
            };
            results.push(result);
        }

        Ok(tonic::Response::new(BatchGetTransactionsResponse::new(
            results,
        )))
    }

    async fn get_checkpoint(
        &self,
        request: tonic::Request<GetCheckpointRequest>,
    ) -> Result<tonic::Response<GetCheckpointResponse>, tonic::Status> {
        let simulacrum = self.simulacrum.read().await;
        let store = simulacrum.store_static();

        let GetCheckpointRequest {
            checkpoint_id,
            read_mask,
            ..
        } = request.into_inner();

        let read_mask = {
            let read_mask = read_mask.unwrap_or_else(|| FieldMask::from_str(READ_MASK_DEFAULT));
            read_mask.validate::<Checkpoint>().map_err(|path| {
                let fv = FieldViolation::new("read_mask")
                    .with_description(format!("invalid read_mask path: {path}"))
                    .with_reason(ErrorReason::FieldInvalid);
                tonic::Status::from(RpcError::from(fv))
            })?;
            FieldMaskTree::from(read_mask)
        };

        let checkpoint = match checkpoint_id {
            Some(CheckpointId::Digest(digest)) => {
                let digest = digest.parse::<CheckpointDigest>().map_err(|e| {
                    let fv = FieldViolation::new("digest")
                        .with_description(format!("invalid digest: {e}"))
                        .with_reason(ErrorReason::FieldInvalid);
                    tonic::Status::from(RpcError::from(fv))
                })?;

                store.get_checkpoint_by_digest(&digest).ok_or_else(|| {
                    let error = CheckpointNotFoundError::digest(digest.into());
                    tonic::Status::from(RpcError::from(error))
                })?
            }
            Some(CheckpointId::SequenceNumber(sequence_number)) => {
                // Default to latest checkpoint
                store
                    .get_checkpoint_by_sequence_number(sequence_number)
                    .ok_or_else(|| {
                        let error = CheckpointNotFoundError::sequence_number(sequence_number);
                        tonic::Status::from(RpcError::from(error))
                    })?
            }
            _ => {
                println!("TODO");
                todo!()
            }
        };

        let mut message = Checkpoint::default();
        let _content_digest = checkpoint.content_digest;

        let summary = checkpoint.data();
        let signature = checkpoint.auth_sig();

        println!("Auth sig: {:?}", signature);

        message.merge(summary, &read_mask);
        message.merge(signature.clone(), &read_mask);
        //
        if read_mask.contains(Checkpoint::CONTENTS_FIELD.name) {
            let checkpoint_contents = store
                .get_checkpoint_contents(&summary.content_digest)
                .ok_or_else(|| {
                    let error = CheckpointNotFoundError::digest((summary.digest()).into());
                    tonic::Status::from(RpcError::from(error))
                })?;
            message.merge(checkpoint_contents, &read_mask);
        }
        //
        // if (read_mask.contains(Checkpoint::TRANSACTIONS_FIELD)
        //     || read_mask.contains(Checkpoint::OBJECTS_FIELD))
        //     && let Some(url) = checkpoint_bucket
        // {
        //     let sequence_number = checkpoint.sequence_number;
        //     let client = create_remote_store_client(url, vec![], 60)?;
        //     let (checkpoint_data, _) =
        //         CheckpointReader::fetch_from_object_store(&client, sequence_number).await?;
        //     let checkpoint = sui_types::full_checkpoint_content::Checkpoint::from(
        //         std::sync::Arc::into_inner(checkpoint_data).unwrap(),
        //     );
        //
        //     message.merge(&checkpoint, &read_mask);
        // }

        Ok(tonic::Response::new(GetCheckpointResponse::new(message)))
    }

    async fn get_epoch(
        &self,
        request: tonic::Request<GetEpochRequest>,
    ) -> Result<tonic::Response<GetEpochResponse>, tonic::Status> {
        let GetEpochRequest {
            epoch: _,
            read_mask,
            ..
        } = request.into_inner();

        let read_mask = {
            let read_mask = read_mask.unwrap_or_else(|| FieldMask::from_str(READ_MASK_DEFAULT));
            read_mask.validate::<Epoch>().map_err(|path| {
                let fv = FieldViolation::new("read_mask")
                    .with_description(format!("invalid read_mask path: {path}"))
                    .with_reason(ErrorReason::FieldInvalid);
                tonic::Status::from(RpcError::from(fv))
            })?;
            FieldMaskTree::from(read_mask)
        };

        let simulacrum = self.simulacrum.read().await;
        let epoch_start_state = simulacrum.epoch_start_state();

        let mut message = Epoch::default();

        if read_mask.contains(Epoch::EPOCH_FIELD.name) {
            message.epoch = Some(epoch_start_state.epoch());
        }
        // if read_mask.contains(Epoch::FIRST_CHECKPOINT_FIELD.name) {
        //     message.first_checkpoint = epoch_info.start_checkpoint;
        // }
        if read_mask.contains(Epoch::LAST_CHECKPOINT_FIELD.name) {
            message.last_checkpoint = simulacrum
                .store()
                .get_highest_checkpint()
                .map(|cp| cp.sequence_number.into());
        }
        if read_mask.contains(Epoch::START_FIELD.name) {
            message.start = Some(sui_rpc_api::proto::timestamp_ms_to_proto(
                epoch_start_state.epoch_start_timestamp_ms(),
            ));
        }
        // if read_mask.contains(Epoch::END_FIELD.name) {
        //     message.end = epoch_info.end_timestamp_ms.map(timestamp_ms_to_proto);
        // }
        if read_mask.contains(Epoch::REFERENCE_GAS_PRICE_FIELD.name) {
            message.reference_gas_price = Some(simulacrum.reference_gas_price());
        }
        if let Some(submask) = read_mask.subtree(Epoch::PROTOCOL_CONFIG_FIELD.name) {
            // Clients request protocol config to determine the active network protocol version.
            // If omitted, they may treat the response as version 0 (proto default), which then
            // fails local protocol-config construction.
            let protocol_config = simulacrum.protocol_config();
            message.protocol_config =
                Some(sui_rpc::proto::sui::rpc::v2::ProtocolConfig::merge_from(
                    protocol_config_to_proto(protocol_config),
                    &submask,
                ));
        }
        if read_mask.contains(Epoch::COMMITTEE_FIELD.name) {
            let committee = epoch_start_state.get_sui_committee();
            let members: Vec<_> = committee
                .members()
                .map(|(key, voting)| {
                    let mut member = ValidatorCommitteeMember::default();
                    member.public_key = Some(key.0.to_vec().into());
                    member.weight = Some(*voting);
                    member
                })
                .collect();

            let mut committee = ValidatorCommittee::default();
            committee.epoch = Some(epoch_start_state.epoch());
            committee.members = members.clone();
            message.committee = Some(committee);
        }
        Ok(tonic::Response::new(GetEpochResponse::new(message)))
    }
}

impl ForkingLedgerService {
    async fn get_object_impl(
        &self,
        object_id: ObjectID,
        version: Option<u64>,
    ) -> Result<sui_types::object::Object, RpcError> {
        println!("get_object_impl: object_id={object_id}, version={version:?}");
        let sim = self.simulacrum.read().await;
        let store = sim.store_static();
        let object = if let Some(version) = version {
            store.get_object_at_version(&object_id, version.into())
        } else {
            println!("Trying to get latest version of object {object_id}");
            store.get_object(&object_id)
        };

        match object {
            Some(obj) => Ok(obj.clone()),
            None => Err(ObjectNotFoundError::new(object_id.into()).into()),
        }
    }

    async fn get_transaction_impl(
        &self,
        digest: sui_types::digests::TransactionDigest,
        read_mask: &FieldMaskTree,
    ) -> Result<ExecutedTransaction, RpcError> {
        use sui_types::storage::ReadStore;

        let sim = self.simulacrum.read().await;
        let store = sim.store_static();

        let transaction = store
            .get_transaction(&digest)
            .ok_or_else(|| TransactionNotFoundError(digest.into()))?;

        let effects = store.get_transaction_effects(&digest);
        let events = store.get_events(&digest);

        let mut message = ExecutedTransaction::default();

        if read_mask.contains(ExecutedTransaction::DIGEST_FIELD.name) {
            message.digest = Some(digest.to_string());
        }

        if let Some(submask) = read_mask.subtree(ExecutedTransaction::TRANSACTION_FIELD.name) {
            let tx = sui_sdk_types::Transaction::try_from(transaction.transaction_data().clone())
                .map_err(|e| {
                RpcError::new(
                    tonic::Code::Internal,
                    format!("Failed to convert transaction: {e}"),
                )
            })?;
            message.transaction = Some(Transaction::merge_from(tx, &submask));
        }

        if let Some(submask) = read_mask.subtree(ExecutedTransaction::SIGNATURES_FIELD.name) {
            message.signatures = transaction
                .tx_signatures()
                .iter()
                .filter_map(|s| {
                    sui_sdk_types::UserSignature::try_from(s.clone())
                        .ok()
                        .map(|s| UserSignature::merge_from(s, &submask))
                })
                .collect();
        }

        if let Some(submask) = read_mask.subtree(ExecutedTransaction::EFFECTS_FIELD.name) {
            if let Some(effects) = effects {
                let eff = effects.clone();
                let effects_sdk: sui_sdk_types::TransactionEffects =
                    eff.try_into().map_err(|e| {
                        RpcError::new(
                            tonic::Code::Internal,
                            format!("Failed to convert effects: {e}"),
                        )
                    })?;
                message.effects = Some(TransactionEffects::merge_from(&effects_sdk, &submask));
            }
        }

        if let Some(submask) = read_mask.subtree(ExecutedTransaction::EVENTS_FIELD.name) {
            if let Some(events) = events {
                let events_sdk: sui_sdk_types::TransactionEvents =
                    events.try_into().map_err(|e| {
                        RpcError::new(
                            tonic::Code::Internal,
                            format!("Failed to convert events: {e}"),
                        )
                    })?;
                message.events = Some(TransactionEvents::merge_from(events_sdk, &submask));
            }
        }

        Ok(message)
    }
}
