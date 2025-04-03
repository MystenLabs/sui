// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use std::convert::Infallible;
use tap::Pipe;
use tonic::server::NamedService;
use tower::Service;

pub(crate) mod v2alpha;
pub(crate) mod v2beta;

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
        S: Service<
                axum::extract::Request,
                Response: axum::response::IntoResponse,
                Error = Infallible,
            > + NamedService
            + Clone
            + Send
            + 'static,
        S::Future: Send + 'static,
        S::Error: Into<BoxError> + Send,
    {
        self.router = self
            .router
            .route_service(&format!("/{}/*rest", S::NAME), svc);
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
        self.get_object(request.into_inner())
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
        self.get_transaction(request.into_inner())
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
        self.get_checkpoint(request.into_inner())
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
        self.get_full_checkpoint(request.into_inner())
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
        self.execute_transaction(request.into_inner())
            .await
            .map(tonic::Response::new)
            .map_err(Into::into)
    }
}
