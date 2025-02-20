// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use crate::proto::types::Bcs;
use http::{Request, Response};
use std::{convert::Infallible, pin::Pin};
use tap::Pipe;
use tonic::{
    body::{boxed, BoxBody},
    server::NamedService,
};
use tower::{Service, ServiceExt};

use crate::subscription::SubscriptionServiceHandle;

pub type BoxError = Box<dyn std::error::Error + Send + Sync + 'static>;

#[derive(Default)]
pub struct Services {
    router: axum::Router,
}

impl Services {
    pub fn new() -> Self {
        Self::default()
    }

    /// Add a new service.
    pub fn add_service<S>(mut self, svc: S) -> Self
    where
        S: Service<Request<BoxBody>, Response = Response<BoxBody>, Error = Infallible>
            + NamedService
            + Clone
            + Send
            + 'static,
        S::Future: Send + 'static,
        S::Error: Into<BoxError> + Send,
    {
        self.router = self.router.route_service(
            &format!("/{}/*rest", S::NAME),
            svc.map_request(|req: Request<axum::body::Body>| req.map(boxed)),
        );
        self
    }

    pub fn into_router(self) -> axum::Router {
        self.router
    }
}

#[tonic::async_trait]
impl crate::proto::node::v2::node_service_server::NodeService for crate::RpcService {
    async fn get_node_info(
        &self,
        _request: tonic::Request<crate::proto::node::v2::GetNodeInfoRequest>,
    ) -> Result<tonic::Response<crate::proto::node::v2::GetNodeInfoResponse>, tonic::Status> {
        self.get_node_info()
            .map(Into::into)
            .map(tonic::Response::new)
            .map_err(Into::into)
    }

    async fn get_committee(
        &self,
        request: tonic::Request<crate::proto::node::v2::GetCommitteeRequest>,
    ) -> std::result::Result<
        tonic::Response<crate::proto::node::v2::GetCommitteeResponse>,
        tonic::Status,
    > {
        let committee = self.get_committee(request.into_inner().epoch)?;

        crate::proto::node::v2::GetCommitteeResponse {
            committee: Some(committee.into()),
        }
        .pipe(tonic::Response::new)
        .pipe(Ok)
    }

    async fn get_object(
        &self,
        request: tonic::Request<crate::proto::node::v2::GetObjectRequest>,
    ) -> std::result::Result<
        tonic::Response<crate::proto::node::v2::GetObjectResponse>,
        tonic::Status,
    > {
        let request = request.into_inner();
        let object_id = request
            .object_id
            .as_ref()
            .ok_or_else(|| tonic::Status::new(tonic::Code::InvalidArgument, "missing object_id"))?
            .try_into()
            .map_err(|_| tonic::Status::new(tonic::Code::InvalidArgument, "invalid object_id"))?;
        let version = request.version;
        let options = if let Some(read_mask) = request.read_mask {
            crate::types::GetObjectOptions::from_read_mask(read_mask)
        } else if let Some(options) = request.options {
            options.into()
        } else {
            Default::default()
        };

        self.get_object(object_id, version, options)
            .map(Into::into)
            .map(tonic::Response::new)
            .map_err(Into::into)
    }

    async fn get_transaction(
        &self,
        request: tonic::Request<crate::proto::node::v2::GetTransactionRequest>,
    ) -> std::result::Result<
        tonic::Response<crate::proto::node::v2::GetTransactionResponse>,
        tonic::Status,
    > {
        let request = request.into_inner();
        let transaction_digest = request
            .digest
            .as_ref()
            .ok_or_else(|| {
                tonic::Status::new(tonic::Code::InvalidArgument, "missing transaction_digest")
            })?
            .try_into()
            .map_err(|_| {
                tonic::Status::new(tonic::Code::InvalidArgument, "invalid transaction_digest")
            })?;

        let options = if let Some(read_mask) = request.read_mask {
            crate::types::GetTransactionOptions::from_read_mask(read_mask)
        } else if let Some(options) = request.options {
            options.into()
        } else {
            Default::default()
        };

        self.get_transaction(transaction_digest, &options)
            .map(Into::into)
            .map(tonic::Response::new)
            .map_err(Into::into)
    }

    async fn get_checkpoint(
        &self,
        request: tonic::Request<crate::proto::node::v2::GetCheckpointRequest>,
    ) -> std::result::Result<
        tonic::Response<crate::proto::node::v2::GetCheckpointResponse>,
        tonic::Status,
    > {
        let request = request.into_inner();
        let checkpoint = match (request.sequence_number, request.digest) {
            (Some(_sequence_number), Some(_digest)) => {
                return Err(tonic::Status::new(
                    tonic::Code::InvalidArgument,
                    "only one of `sequence_number` or `digest` can be provided",
                ))
            }
            (Some(sequence_number), None) => Some(
                crate::service::checkpoints::CheckpointId::SequenceNumber(sequence_number),
            ),
            (None, Some(digest)) => Some(crate::service::checkpoints::CheckpointId::Digest(
                (&digest).try_into().map_err(|_| {
                    tonic::Status::new(tonic::Code::InvalidArgument, "invalid digest")
                })?,
            )),
            (None, None) => None,
        };

        let options = if let Some(read_mask) = request.read_mask {
            crate::types::GetCheckpointOptions::from_read_mask(read_mask)
        } else if let Some(options) = request.options {
            options.into()
        } else {
            Default::default()
        };

        self.get_checkpoint(checkpoint, options)
            .map(Into::into)
            .map(tonic::Response::new)
            .map_err(Into::into)
    }

    async fn get_full_checkpoint(
        &self,
        request: tonic::Request<crate::proto::node::v2::GetFullCheckpointRequest>,
    ) -> std::result::Result<
        tonic::Response<crate::proto::node::v2::GetFullCheckpointResponse>,
        tonic::Status,
    > {
        let request = request.into_inner();
        let checkpoint = match (request.sequence_number, request.digest) {
            (Some(_sequence_number), Some(_digest)) => {
                return Err(tonic::Status::new(
                    tonic::Code::InvalidArgument,
                    "only one of `sequence_number` or `digest` can be provided",
                ))
            }
            (Some(sequence_number), None) => {
                crate::service::checkpoints::CheckpointId::SequenceNumber(sequence_number)
            }

            (None, Some(digest)) => {
                crate::service::checkpoints::CheckpointId::Digest((&digest).try_into().map_err(
                    |_| tonic::Status::new(tonic::Code::InvalidArgument, "invalid digest"),
                )?)
            }
            (None, None) => {
                return Err(tonic::Status::new(
                    tonic::Code::InvalidArgument,
                    "must provided one of `sequence_number` or `digest`",
                ))
            }
        };

        let options = if let Some(read_mask) = request.read_mask {
            crate::types::GetFullCheckpointOptions::from_read_mask(read_mask)
        } else if let Some(options) = request.options {
            options.into()
        } else {
            Default::default()
        };

        self.get_full_checkpoint(checkpoint, &options)
            .map(Into::into)
            .map(tonic::Response::new)
            .map_err(Into::into)
    }

    async fn execute_transaction(
        &self,
        request: tonic::Request<crate::proto::node::v2::ExecuteTransactionRequest>,
    ) -> std::result::Result<
        tonic::Response<crate::proto::node::v2::ExecuteTransactionResponse>,
        tonic::Status,
    > {
        let request = request.into_inner();
        let transaction = match (request.transaction, request.transaction_bcs) {
            (Some(_), Some(_)) => {
                return Err(tonic::Status::new(
                    tonic::Code::InvalidArgument,
                    "only one of `transaction` or `transaction_bcs` can be provided",
                ))
            }
            (Some(transaction), None) => (&transaction).try_into().map_err(|e| {
                tonic::Status::new(
                    tonic::Code::InvalidArgument,
                    format!("invalid transaction: {e}"),
                )
            })?,

            (None, Some(bcs)) => bcs::from_bytes(bcs.bcs()).map_err(|_| {
                tonic::Status::new(tonic::Code::InvalidArgument, "invalid transaction bcs")
            })?,

            (None, None) => {
                return Err(tonic::Status::new(
                    tonic::Code::InvalidArgument,
                    "one of `transaction` or `transaction_bcs` must be provided",
                ))
            }
        };
        let mut signatures: Vec<sui_sdk_types::UserSignature> = Vec::new();

        if !request.signatures.is_empty() {
            let from_proto_signatures = request
                .signatures
                .iter()
                .map(TryInto::try_into)
                .collect::<Result<Vec<_>, _>>()
                .map_err(|e| {
                    tonic::Status::new(
                        tonic::Code::InvalidArgument,
                        format!("invalid signature: {e}"),
                    )
                })?;

            signatures.extend(from_proto_signatures);
        }

        if !request.signatures_bytes.is_empty() {
            let from_bytes_signatures = request
                .signatures_bytes
                .iter()
                .map(|bytes| sui_sdk_types::UserSignature::from_bytes(bytes))
                .collect::<Result<Vec<_>, _>>()
                .map_err(|e| {
                    tonic::Status::new(
                        tonic::Code::InvalidArgument,
                        format!("invalid signature: {e}"),
                    )
                })?;

            signatures.extend(from_bytes_signatures);
        }

        let signed_transaction = sui_sdk_types::SignedTransaction {
            transaction,
            signatures,
        };

        let options = request.options.unwrap_or_default().into();

        self.execute_transaction(signed_transaction, None, &options)
            .await
            .map(Into::into)
            .map(tonic::Response::new)
            .map_err(Into::into)
    }
}

use crate::proto::node::v2alpha::SubscribeCheckpointsResponse;

#[tonic::async_trait]
impl crate::proto::node::v2alpha::subscription_service_server::SubscriptionService
    for SubscriptionServiceHandle
{
    /// Server streaming response type for the SubscribeCheckpoints method.
    type SubscribeCheckpointsStream = Pin<
        Box<
            dyn tokio_stream::Stream<Item = Result<SubscribeCheckpointsResponse, tonic::Status>>
                + Send,
        >,
    >;

    async fn subscribe_checkpoints(
        &self,
        request: tonic::Request<crate::proto::node::v2alpha::SubscribeCheckpointsRequest>,
    ) -> Result<tonic::Response<Self::SubscribeCheckpointsStream>, tonic::Status> {
        let read_mask = request.into_inner().read_mask.unwrap_or_default();

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

                let checkpoint = apply_checkpoint_read_mask(&read_mask, &checkpoint);
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

// Go through all of the fields of the checkpoint and apply the provided 'options'.
//
// This function assumes that the provided checkpoint has all fields populated and that applying
// the requested options is a matter of removing the data that the request didn't ask for.
fn apply_checkpoint_read_mask(
    read_mask: &prost_types::FieldMask,
    checkpoint: &crate::proto::node::v2::GetFullCheckpointResponse,
) -> crate::proto::node::v2::GetFullCheckpointResponse {
    let mut response = crate::proto::node::v2::GetFullCheckpointResponse::default();

    for path in &read_mask.paths {
        let mut components = path.split('.');
        let Some(component) = components.next() else {
            continue;
        };

        match component {
            "sequence_number" => response.sequence_number = checkpoint.sequence_number,
            "digest" => response.digest = checkpoint.digest.clone(),
            "summary" => response.summary = checkpoint.summary.clone(),
            "summary_bcs" => response.summary_bcs = checkpoint.summary_bcs.clone(),
            "signature" => response.signature = checkpoint.signature.clone(),
            "contents" => response.contents = checkpoint.contents.clone(),
            "contents_bcs" => response.contents_bcs = checkpoint.contents_bcs.clone(),
            "transactions" => {
                let Some(component) = components.next() else {
                    response.transactions = checkpoint.transactions.clone();
                    continue;
                };
                if response.transactions.len() != checkpoint.transactions.len() {
                    response.transactions = vec![Default::default(); checkpoint.transactions.len()];
                }

                for (src, dst) in checkpoint
                    .transactions
                    .iter()
                    .zip(response.transactions.iter_mut())
                {
                    match component {
                        "digest" => dst.digest = src.digest.clone(),
                        "transaction" => dst.transaction = src.transaction.clone(),
                        "transaction_bcs" => dst.transaction_bcs = src.transaction_bcs.clone(),
                        "effects" => dst.effects = src.effects.clone(),
                        "effects_bcs" => dst.effects_bcs = src.effects_bcs.clone(),
                        "events" => dst.events = src.events.clone(),
                        "events_bcs" => dst.events_bcs = src.events_bcs.clone(),
                        "input_objects" => {
                            let Some(component) = components.clone().next() else {
                                dst.input_objects = src.input_objects.clone();
                                continue;
                            };
                            if dst.input_objects.len() != src.input_objects.len() {
                                dst.input_objects =
                                    vec![Default::default(); src.input_objects.len()];
                            }

                            for (src, dst) in
                                src.input_objects.iter().zip(dst.input_objects.iter_mut())
                            {
                                match component {
                                    "object" => dst.object = src.object.clone(),
                                    "object_bcs" => dst.object_bcs = src.object_bcs.clone(),
                                    // Ignore unknown field
                                    _ => {}
                                }
                            }
                        }
                        "output_objects" => {
                            let Some(component) = components.clone().next() else {
                                dst.output_objects = src.output_objects.clone();
                                continue;
                            };
                            if dst.output_objects.len() != src.output_objects.len() {
                                dst.output_objects =
                                    vec![Default::default(); src.output_objects.len()];
                            }

                            for (src, dst) in
                                src.output_objects.iter().zip(dst.output_objects.iter_mut())
                            {
                                match component {
                                    "object" => dst.object = src.object.clone(),
                                    "object_bcs" => dst.object_bcs = src.object_bcs.clone(),
                                    // Ignore unknown field
                                    _ => {}
                                }
                            }
                        }
                        // Ignore unknown field
                        _ => {}
                    }
                }
            }
            // Ignore unknown field
            _ => {}
        }
    }

    response
}

#[tonic::async_trait]
impl crate::proto::node::v2alpha::node_service_server::NodeService for crate::RpcService {
    async fn get_coin_info(
        &self,
        request: tonic::Request<crate::proto::node::v2alpha::GetCoinInfoRequest>,
    ) -> Result<tonic::Response<crate::proto::node::v2alpha::GetCoinInfoResponse>, tonic::Status>
    {
        self.get_coin_info(request.into_inner())
            .map(tonic::Response::new)
            .map_err(Into::into)
    }

    async fn list_dynamic_fields(
        &self,
        request: tonic::Request<crate::proto::node::v2alpha::ListDynamicFieldsRequest>,
    ) -> std::result::Result<
        tonic::Response<crate::proto::node::v2alpha::ListDynamicFieldsResponse>,
        tonic::Status,
    > {
        self.list_dynamic_fields(request.into_inner())
            .map(tonic::Response::new)
            .map_err(Into::into)
    }

    async fn list_account_objects(
        &self,
        request: tonic::Request<crate::proto::node::v2alpha::ListAccountObjectsRequest>,
    ) -> std::result::Result<
        tonic::Response<crate::proto::node::v2alpha::ListAccountObjectsResponse>,
        tonic::Status,
    > {
        self.list_account_objects(request.into_inner())
            .map(tonic::Response::new)
            .map_err(Into::into)
    }

    async fn get_protocol_config(
        &self,
        request: tonic::Request<crate::proto::node::v2alpha::GetProtocolConfigRequest>,
    ) -> std::result::Result<
        tonic::Response<crate::proto::node::v2alpha::GetProtocolConfigResponse>,
        tonic::Status,
    > {
        self.get_protocol_config(request.into_inner())
            .map(tonic::Response::new)
            .map_err(Into::into)
    }

    async fn get_gas_info(
        &self,
        request: tonic::Request<crate::proto::node::v2alpha::GetGasInfoRequest>,
    ) -> std::result::Result<
        tonic::Response<crate::proto::node::v2alpha::GetGasInfoResponse>,
        tonic::Status,
    > {
        self.get_gas_info(request.into_inner())
            .map(tonic::Response::new)
            .map_err(Into::into)
    }

    async fn simulate_transaction(
        &self,
        request: tonic::Request<crate::proto::node::v2alpha::SimulateTransactionRequest>,
    ) -> std::result::Result<
        tonic::Response<crate::proto::node::v2alpha::SimulateTransactionResponse>,
        tonic::Status,
    > {
        let request = request.into_inner();
        //TODO use provided read_mask
        let parameters = crate::types::SimulateTransactionQueryParameters {
            balance_changes: false,
            input_objects: false,
            output_objects: false,
        };
        let transaction = if let Some(bcs) = request.transaction_bcs {
            bcs::from_bytes(bcs.bcs()).map_err(|_| {
                tonic::Status::new(tonic::Code::InvalidArgument, "invalid transaction bcs")
            })?
        } else {
            return Err(tonic::Status::new(
                tonic::Code::InvalidArgument,
                "`transaction_bcs` must be provided",
            ));
        };

        let response = self.simulate_transaction(&parameters, transaction)?;

        let balance_changes = response
            .balance_changes
            .map(|balance_changes| balance_changes.into_iter().map(Into::into).collect())
            .unwrap_or_default();
        let response = crate::proto::node::v2alpha::SimulateTransactionResponse {
            effects_bcs: Some(Bcs::serialize(&response.effects).unwrap()),
            events_bcs: response
                .events
                .map(|events| Bcs::serialize(&events))
                .transpose()
                .unwrap(),
            balance_changes,
        };
        Ok(tonic::Response::new(response))
    }

    async fn resolve_transaction(
        &self,
        request: tonic::Request<crate::proto::node::v2alpha::ResolveTransactionRequest>,
    ) -> std::result::Result<
        tonic::Response<crate::proto::node::v2alpha::ResolveTransactionResponse>,
        tonic::Status,
    > {
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

        let simulation = response.simulation.map(|simulation| {
            let balance_changes = simulation
                .balance_changes
                .map(|balance_changes| balance_changes.into_iter().map(Into::into).collect())
                .unwrap_or_default();
            crate::proto::node::v2alpha::SimulateTransactionResponse {
                effects_bcs: Some(Bcs::serialize(&simulation.effects).unwrap()),
                events_bcs: simulation
                    .events
                    .map(|events| Bcs::serialize(&events))
                    .transpose()
                    .unwrap(),
                balance_changes,
            }
        });

        let response = crate::proto::node::v2alpha::ResolveTransactionResponse {
            transaction_bcs: Some(Bcs::serialize(&response.transaction).unwrap()),
            simulation,
        };

        Ok(tonic::Response::new(response))
    }
}
