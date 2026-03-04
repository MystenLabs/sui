// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use crate::RpcService;
use crate::grpc::v2::list_authenticated_events;
use sui_rpc::proto::sui::rpc::v2::event_service_server::EventService;
use sui_rpc::proto::sui::rpc::v2::{
    ListAuthenticatedEventsRequest, ListAuthenticatedEventsResponse,
};

#[tonic::async_trait]
impl EventService for RpcService {
    async fn list_authenticated_events(
        &self,
        request: tonic::Request<ListAuthenticatedEventsRequest>,
    ) -> Result<tonic::Response<ListAuthenticatedEventsResponse>, tonic::Status> {
        let req = request.into_inner();
        let resp: ListAuthenticatedEventsResponse =
            list_authenticated_events::list_authenticated_events(self, req)
                .map_err(tonic::Status::from)?;
        Ok(tonic::Response::new(resp))
    }
}
