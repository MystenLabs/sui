// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0
use anemo_tower::callback::{MakeCallbackHandler, ResponseHandler};
use sui_macros::fail_point;

#[derive(Clone, Default)]
pub struct FailpointsMakeCallbackHandler {}

impl FailpointsMakeCallbackHandler {
    pub fn new() -> Self {
        Self {}
    }
}

impl MakeCallbackHandler for FailpointsMakeCallbackHandler {
    type Handler = FailpointsResponseHandler;

    fn make_handler(&self, _request: &anemo::Request<bytes::Bytes>) -> Self::Handler {
        FailpointsResponseHandler {}
    }
}

pub struct FailpointsResponseHandler {}

impl ResponseHandler for FailpointsResponseHandler {
    fn on_response(self, _response: &anemo::Response<bytes::Bytes>) {
        fail_point!("narwhal-rpc-response");
    }

    fn on_error<E>(self, _error: &E) {}
}
