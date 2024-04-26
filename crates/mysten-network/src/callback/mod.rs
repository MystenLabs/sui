// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use http::{request, response};

mod future;
mod layer;
mod service;

pub use self::{future::ResponseFuture, layer::CallbackLayer, service::Callback};

pub trait MakeCallbackHandler {
    type Handler: ResponseHandler;

    fn make_handler(&self, request: &request::Parts) -> Self::Handler;
}

pub trait ResponseHandler {
    fn on_response(self, response: &response::Parts);
    fn on_error<E>(self, error: &E);
}
