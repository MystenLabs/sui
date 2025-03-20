// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use std::pin::Pin;

use tap::Pipe;

use crate::field_mask::FieldMaskTree;
use crate::message::MessageMergeFrom;
use crate::proto::google::rpc::bad_request::FieldViolation;
use crate::proto::rpc::v2alpha::live_data_service_server::LiveDataService;
use crate::proto::rpc::v2alpha::subscription_service_server::SubscriptionService;
use crate::proto::rpc::v2alpha::GetCoinInfoRequest;
use crate::proto::rpc::v2alpha::GetCoinInfoResponse;
use crate::proto::rpc::v2alpha::ListDynamicFieldsRequest;
use crate::proto::rpc::v2alpha::ListDynamicFieldsResponse;
use crate::proto::rpc::v2alpha::ListOwnedObjectsRequest;
use crate::proto::rpc::v2alpha::ListOwnedObjectsResponse;
use crate::proto::rpc::v2alpha::ResolveTransactionRequest;
use crate::proto::rpc::v2alpha::ResolveTransactionResponse;
use crate::proto::rpc::v2alpha::SimulateTransactionRequest;
use crate::proto::rpc::v2alpha::SimulateTransactionResponse;
use crate::proto::rpc::v2alpha::SubscribeCheckpointsRequest;
use crate::proto::rpc::v2alpha::SubscribeCheckpointsResponse;
use crate::proto::rpc::v2beta::Checkpoint;
use crate::proto::rpc::v2beta::ExecutedTransaction;
use crate::proto::rpc::v2beta::Transaction;
use crate::proto::rpc::v2beta::TransactionEffects;
use crate::proto::rpc::v2beta::TransactionEvents;
use crate::subscription::SubscriptionServiceHandle;
use crate::ErrorReason;
use crate::RpcError;
use crate::RpcService;

#[tonic::async_trait]
impl SubscriptionService for SubscriptionServiceHandle {
    /// Server streaming response type for the SubscribeCheckpoints method.
    type SubscribeCheckpointsStream = Pin<
        Box<
            dyn tokio_stream::Stream<Item = Result<SubscribeCheckpointsResponse, tonic::Status>>
                + Send,
        >,
    >;

    async fn subscribe_checkpoints(
        &self,
        request: tonic::Request<SubscribeCheckpointsRequest>,
    ) -> Result<tonic::Response<Self::SubscribeCheckpointsStream>, tonic::Status> {
        let read_mask = request.into_inner().read_mask.unwrap_or_default();
        let read_mask = FieldMaskTree::from(read_mask);

        let Some(mut receiver) = self.register_subscription().await else {
            return Err(tonic::Status::unavailable(
                "too many existing subscriptions",
            ));
        };

        let response = Box::pin(async_stream::stream! {
            while let Some(checkpoint) = receiver.recv().await {
                let Some(cursor) = checkpoint.sequence_number else {
                    yield Err(tonic::Status::internal("unable to determine cursor"));
                    break;
                };

                let checkpoint = Checkpoint::merge_from(checkpoint.as_ref(), &read_mask);
                let response = SubscribeCheckpointsResponse {
                    cursor: Some(cursor),
                    checkpoint: Some(checkpoint),
                };

                yield Ok(response);
            }
        });

        Ok(tonic::Response::new(response))
    }
}

mod get_coin_info;
mod list_dynamic_fields;
mod list_owned_objects;

#[tonic::async_trait]
impl LiveDataService for RpcService {
    async fn list_dynamic_fields(
        &self,
        request: tonic::Request<ListDynamicFieldsRequest>,
    ) -> Result<tonic::Response<ListDynamicFieldsResponse>, tonic::Status> {
        list_dynamic_fields::list_dynamic_fields(self, request.into_inner())
            .map(tonic::Response::new)
            .map_err(Into::into)
    }

    async fn list_owned_objects(
        &self,
        request: tonic::Request<ListOwnedObjectsRequest>,
    ) -> Result<tonic::Response<ListOwnedObjectsResponse>, tonic::Status> {
        list_owned_objects::list_owned_objects(self, request.into_inner())
            .map(tonic::Response::new)
            .map_err(Into::into)
    }

    async fn get_coin_info(
        &self,
        request: tonic::Request<GetCoinInfoRequest>,
    ) -> Result<tonic::Response<GetCoinInfoResponse>, tonic::Status> {
        get_coin_info::get_coin_info(self, request.into_inner())
            .map(tonic::Response::new)
            .map_err(Into::into)
    }

    async fn simulate_transaction(
        &self,
        request: tonic::Request<SimulateTransactionRequest>,
    ) -> Result<tonic::Response<SimulateTransactionResponse>, tonic::Status> {
        let request = request.into_inner();
        let read_mask = FieldMaskTree::new_wildcard();
        //TODO use provided read_mask
        let parameters = crate::types::SimulateTransactionQueryParameters {
            balance_changes: false,
            input_objects: false,
            output_objects: false,
        };
        let transaction = request
            .transaction
            .as_ref()
            .ok_or_else(|| {
                FieldViolation::new("transaction").with_reason(ErrorReason::FieldMissing)
            })
            .map_err(RpcError::from)?
            .pipe(sui_sdk_types::Transaction::try_from)
            .map_err(|e| {
                FieldViolation::new("transaction")
                    .with_description(format!("invalid transaction: {e}"))
                    .with_reason(ErrorReason::FieldInvalid)
            })
            .map_err(RpcError::from)?;

        let response = self.simulate_transaction(&parameters, transaction)?;

        let balance_changes = response
            .balance_changes
            .map(|balance_changes| balance_changes.into_iter().map(Into::into).collect())
            .unwrap_or_default();
        let response = crate::proto::rpc::v2alpha::SimulateTransactionResponse {
            transaction: Some(ExecutedTransaction {
                effects: Some(TransactionEffects::merge_from(
                    &response.effects,
                    &read_mask,
                )),
                events: response
                    .events
                    .map(|events| TransactionEvents::merge_from(events, &read_mask)),
                balance_changes,
                ..Default::default()
            }),
        };
        Ok(tonic::Response::new(response))
    }

    async fn resolve_transaction(
        &self,
        request: tonic::Request<ResolveTransactionRequest>,
    ) -> Result<tonic::Response<ResolveTransactionResponse>, tonic::Status> {
        let request = request.into_inner();
        let read_mask = request.read_mask.unwrap_or_default();
        //TODO use provided read_mask
        let simulate_parameters = crate::types::SimulateTransactionQueryParameters {
            balance_changes: false,
            input_objects: false,
            output_objects: false,
        };
        let parameters = crate::types::ResolveTransactionQueryParameters {
            simulate: read_mask
                .paths
                .iter()
                .any(|path| path.starts_with("simulation")),
            simulate_transaction_parameters: simulate_parameters,
        };
        let unresolved_transaction = serde_json::from_str(
            &request.unresolved_transaction.unwrap_or_default(),
        )
        .map_err(|_| {
            tonic::Status::new(
                tonic::Code::InvalidArgument,
                "invalid unresolved_transaction",
            )
        })?;

        let response = self.resolve_transaction(parameters, unresolved_transaction)?;

        let read_mask = FieldMaskTree::new_wildcard();
        let simulation = response.simulation.map(|simulation| {
            let balance_changes = simulation
                .balance_changes
                .map(|balance_changes| balance_changes.into_iter().map(Into::into).collect())
                .unwrap_or_default();
            crate::proto::rpc::v2alpha::SimulateTransactionResponse {
                transaction: Some(ExecutedTransaction {
                    effects: Some(TransactionEffects::merge_from(
                        &simulation.effects,
                        &read_mask,
                    )),
                    events: simulation
                        .events
                        .map(|events| TransactionEvents::merge_from(events, &read_mask)),
                    balance_changes,
                    ..Default::default()
                }),
            }
        });

        let response = crate::proto::rpc::v2alpha::ResolveTransactionResponse {
            transaction: Some(Transaction::merge_from(response.transaction, &read_mask)),
            simulation,
        };

        Ok(tonic::Response::new(response))
    }
}
