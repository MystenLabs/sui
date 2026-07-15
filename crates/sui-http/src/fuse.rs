// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use std::{
    future::Future,
    pin::Pin,
    task::{Context, Poll},
};

// From `futures-util` crate
// LICENSE: MIT or Apache-2.0
// A future which only yields `Poll::Ready` once, and thereafter yields `Poll::Pending`.
pin_project_lite::pin_project! {
    pub struct Fuse<F> {
        #[pin]
        inner: Option<F>,
    }
}

impl<F> Fuse<F> {
    pub fn new(future: F) -> Self {
        Self {
            inner: Some(future),
        }
    }
}

impl<F> Future for Fuse<F>
where
    F: Future,
{
    type Output = F::Output;

    fn poll(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Self::Output> {
        match self.as_mut().project().inner.as_pin_mut() {
            Some(fut) => fut.poll(cx).map(|output| {
                self.project().inner.set(None);
                output
            }),
            None => Poll::Pending,
        }
    }
}
