// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use super::ResponseBody;
use super::ResponseHandler;
use http::Response;
use pin_project_lite::pin_project;
use std::{
    future::Future,
    pin::Pin,
    task::{Context, Poll},
};

pin_project! {
    /// Response future for [`Callback`].
    ///
    /// [`Callback`]: super::Callback
    pub struct ResponseFuture<F, ResponseHandler> {
        #[pin]
        pub(crate) inner: F,
        pub(crate) handler: Option<ResponseHandler>,
    }
}

impl<Fut, B, E, ResponseHandlerT> Future for ResponseFuture<Fut, ResponseHandlerT>
where
    Fut: Future<Output = Result<Response<B>, E>>,
    B: http_body::Body<Error: std::fmt::Display + 'static>,
    E: std::fmt::Display + 'static,
    ResponseHandlerT: ResponseHandler,
{
    type Output = Result<Response<ResponseBody<B, ResponseHandlerT>>, E>;

    fn poll(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Self::Output> {
        let this = self.project();
        let result = futures::ready!(this.inner.poll(cx));
        let mut handler = this.handler.take().unwrap();

        let result = match result {
            Ok(response) => {
                let (head, body) = response.into_parts();
                handler.on_response(&head);
                Ok(Response::from_parts(
                    head,
                    ResponseBody {
                        inner: body,
                        handler,
                    },
                ))
            }
            Err(error) => {
                handler.on_error(&error);
                Err(error)
            }
        };

        Poll::Ready(result)
    }
}
