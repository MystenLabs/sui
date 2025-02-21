// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use http::{request, response, HeaderMap};

mod body;
mod future;
mod layer;
mod service;

pub use self::{
    body::ResponseBody, future::ResponseFuture, layer::CallbackLayer, service::Callback,
};

pub trait MakeCallbackHandler {
    type Handler: ResponseHandler;

    fn make_handler(&self, request: &request::Parts) -> Self::Handler;
}

pub trait ResponseHandler {
    fn on_response(&mut self, response: &response::Parts);
    fn on_error<E>(&mut self, error: &E)
    where
        E: std::fmt::Display + 'static;

    fn on_body_chunk<B>(&mut self, _chunk: &B)
    where
        B: bytes::Buf,
    {
        // do nothing
    }

    fn on_end_of_stream(&mut self, _trailers: Option<&HeaderMap>) {
        // do nothing
    }
}
