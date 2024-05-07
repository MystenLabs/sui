// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use super::{Callback, MakeCallbackHandler};
use tower::Layer;

/// [`Layer`] that adds callbacks to a [`Service`].
///
/// See the [module docs](crate::callback) for more details.
///
/// [`Layer`]: tower::layer::Layer
/// [`Service`]: tower::Service
#[derive(Debug, Copy, Clone)]
pub struct CallbackLayer<M> {
    pub(crate) make_handler: M,
}

impl<M> CallbackLayer<M> {
    /// Create a new [`CallbackLayer`] using the given [`MakeCallbackHandler`].
    pub fn new(make_handler: M) -> Self
    where
        M: MakeCallbackHandler,
    {
        Self { make_handler }
    }
}

impl<S, M> Layer<S> for CallbackLayer<M>
where
    M: Clone,
{
    type Service = Callback<S, M>;

    fn layer(&self, inner: S) -> Self::Service {
        Callback {
            inner,
            make_callback_handler: self.make_handler.clone(),
        }
    }
}
