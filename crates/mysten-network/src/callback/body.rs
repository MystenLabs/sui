// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use super::ResponseHandler;
use http_body::{Body, Frame};
use pin_project_lite::pin_project;
use std::{
    fmt,
    pin::Pin,
    task::{ready, Context, Poll},
};

pin_project! {
    /// Response body for [`Callback`].
    ///
    /// [`Callback`]: super::Callback
    pub struct ResponseBody<B, ResponseHandler> {
        #[pin]
        pub(crate) inner: B,
        pub(crate) handler: ResponseHandler,
    }
}

impl<B, ResponseHandlerT> Body for ResponseBody<B, ResponseHandlerT>
where
    B: Body,
    B::Error: fmt::Display + 'static,
    ResponseHandlerT: ResponseHandler,
{
    type Data = B::Data;
    type Error = B::Error;

    fn poll_frame(
        self: Pin<&mut Self>,
        cx: &mut Context<'_>,
    ) -> Poll<Option<Result<http_body::Frame<Self::Data>, Self::Error>>> {
        let this = self.project();
        let result = ready!(this.inner.poll_frame(cx));

        match result {
            Some(Ok(frame)) => {
                let frame = match frame.into_data() {
                    Ok(chunk) => {
                        this.handler.on_body_chunk(&chunk);
                        Frame::data(chunk)
                    }
                    Err(frame) => frame,
                };

                let frame = match frame.into_trailers() {
                    Ok(trailers) => {
                        this.handler.on_end_of_stream(Some(&trailers));
                        Frame::trailers(trailers)
                    }
                    Err(frame) => frame,
                };

                Poll::Ready(Some(Ok(frame)))
            }
            Some(Err(err)) => {
                this.handler.on_error(&err);

                Poll::Ready(Some(Err(err)))
            }
            None => {
                this.handler.on_end_of_stream(None);

                Poll::Ready(None)
            }
        }
    }

    fn is_end_stream(&self) -> bool {
        self.inner.is_end_stream()
    }

    fn size_hint(&self) -> http_body::SizeHint {
        self.inner.size_hint()
    }
}
