// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use http::{Request, Response};
use std::convert::Infallible;
use tap::Pipe;
use tonic::{
    body::{boxed, BoxBody},
    server::NamedService,
};
use tower::{Service, ServiceExt};

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
impl crate::proto::node::node_server::Node for crate::RpcService {
    async fn get_node_info(
        &self,
        _request: tonic::Request<()>,
    ) -> Result<tonic::Response<crate::proto::node::GetNodeInfoResponse>, tonic::Status> {
        self.get_node_info()
            .map(Into::into)
            .map(tonic::Response::new)
            .map_err(Into::into)
    }

    async fn get_committee(
        &self,
        request: tonic::Request<crate::proto::node::GetCommitteeRequest>,
    ) -> std::result::Result<tonic::Response<crate::proto::node::GetCommitteeResponse>, tonic::Status>
    {
        let committee = self.get_committee(request.into_inner().epoch)?;

        crate::proto::node::GetCommitteeResponse {
            committee: Some(committee.into()),
        }
        .pipe(tonic::Response::new)
        .pipe(Ok)
    }

    async fn get_object(
        &self,
        request: tonic::Request<crate::proto::node::GetObjectRequest>,
    ) -> std::result::Result<tonic::Response<crate::proto::node::GetObjectResponse>, tonic::Status>
    {
        let request = request.into_inner();
        let object_id = request
            .object_id
            .as_ref()
            .ok_or_else(|| tonic::Status::new(tonic::Code::InvalidArgument, "missing object_id"))?
            .try_into()
            .map_err(|_| tonic::Status::new(tonic::Code::InvalidArgument, "invalid object_id"))?;
        let version = request.version;
        let options = request.options.unwrap_or_default().into();

        self.get_object(object_id, version, options)
            .map(Into::into)
            .map(tonic::Response::new)
            .map_err(Into::into)
    }

    async fn get_transaction(
        &self,
        request: tonic::Request<crate::proto::node::GetTransactionRequest>,
    ) -> std::result::Result<
        tonic::Response<crate::proto::node::GetTransactionResponse>,
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

        let options = request.options.unwrap_or_default().into();

        self.get_transaction(transaction_digest, &options)
            .map(Into::into)
            .map(tonic::Response::new)
            .map_err(Into::into)
    }

    async fn get_checkpoint(
        &self,
        request: tonic::Request<crate::proto::node::GetCheckpointRequest>,
    ) -> std::result::Result<
        tonic::Response<crate::proto::node::GetCheckpointResponse>,
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

        let options = request.options.unwrap_or_default().into();

        self.get_checkpoint(checkpoint, options)
            .map(Into::into)
            .map(tonic::Response::new)
            .map_err(Into::into)
    }

    async fn get_full_checkpoint(
        &self,
        request: tonic::Request<crate::proto::node::GetFullCheckpointRequest>,
    ) -> std::result::Result<
        tonic::Response<crate::proto::node::GetFullCheckpointResponse>,
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

        let options = request.options.unwrap_or_default().into();

        self.get_full_checkpoint(checkpoint, &options)
            .map(Into::into)
            .map(tonic::Response::new)
            .map_err(Into::into)
    }

    async fn execute_transaction(
        &self,
        request: tonic::Request<crate::proto::node::ExecuteTransactionRequest>,
    ) -> std::result::Result<
        tonic::Response<crate::proto::node::ExecuteTransactionResponse>,
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
        let mut signatures: Vec<sui_sdk_types::types::UserSignature> = Vec::new();

        if let Some(proto_signatures) = request.signatures {
            let from_proto_signatures = proto_signatures
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

        if let Some(signatures_bytes) = request.signatures_bytes {
            let from_bytes_signatures = signatures_bytes
                .signatures
                .iter()
                .map(|bytes| sui_sdk_types::types::UserSignature::from_bytes(bytes))
                .collect::<Result<Vec<_>, _>>()
                .map_err(|e| {
                    tonic::Status::new(
                        tonic::Code::InvalidArgument,
                        format!("invalid signature: {e}"),
                    )
                })?;

            signatures.extend(from_bytes_signatures);
        }

        let signed_transaction = sui_sdk_types::types::SignedTransaction {
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
