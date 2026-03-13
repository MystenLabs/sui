// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use sui_rpc::proto::sui::rpc::v2::get_checkpoint_request::CheckpointId;
use sui_rpc::proto::sui::rpc::v2::{
    ExecuteTransactionRequest, GetCheckpointRequest, ListOwnedObjectsRequest,
    SimulateTransactionRequest, SubscribeCheckpointsRequest,
};
use sui_types::base_types::{ObjectID, SuiAddress};

pub fn deterministic_address(byte: u8) -> SuiAddress {
    let mut bytes = [0u8; ObjectID::LENGTH];
    bytes[ObjectID::LENGTH - 1] = byte;
    SuiAddress::from(ObjectID::new(bytes))
}

pub fn deterministic_missing_object_id(byte: u8) -> ObjectID {
    let mut bytes = [0u8; ObjectID::LENGTH];
    bytes[0] = 0xff;
    bytes[ObjectID::LENGTH - 1] = byte;
    ObjectID::new(bytes)
}

pub fn checkpoint_request(sequence_number: u64) -> GetCheckpointRequest {
    let mut request = GetCheckpointRequest::default();
    request.checkpoint_id = Some(CheckpointId::SequenceNumber(sequence_number));
    request
}

pub fn list_owned_objects_request(owner: SuiAddress, page_size: u32) -> ListOwnedObjectsRequest {
    let mut request = ListOwnedObjectsRequest::default();
    request.owner = Some(owner.to_string());
    request.page_size = Some(page_size);
    request
}

pub fn list_owned_objects_missing_owner_request() -> ListOwnedObjectsRequest {
    ListOwnedObjectsRequest::default()
}

pub fn subscribe_checkpoints_request() -> SubscribeCheckpointsRequest {
    SubscribeCheckpointsRequest::default()
}

pub fn execute_transaction_missing_transaction_request() -> ExecuteTransactionRequest {
    ExecuteTransactionRequest::default()
}

pub fn simulate_transaction_missing_transaction_request() -> SimulateTransactionRequest {
    SimulateTransactionRequest::default()
}
