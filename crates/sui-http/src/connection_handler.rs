// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use std::{pin::pin, time::Duration};

use http::{Request, Response};
use tracing::{debug, trace};

use crate::{fuse::Fuse, ActiveConnections, BoxError, ConnectionId};

// This is moved to its own function as a way to get around
// https://github.com/rust-lang/rust/issues/102211
pub async fn serve_connection<IO, S, B, C>(
    hyper_io: IO,
    hyper_svc: S,
    builder: hyper_util::server::conn::auto::Builder<hyper_util::rt::TokioExecutor>,
    graceful_shutdown_token: tokio_util::sync::CancellationToken,
    max_connection_age: Option<Duration>,
    on_connection_close: C,
) where
    B: http_body::Body + Send + 'static,
    B::Data: Send,
    B::Error: Into<BoxError>,
    IO: hyper::rt::Read + hyper::rt::Write + Send + Unpin + 'static,
    S: hyper::service::Service<Request<hyper::body::Incoming>, Response = Response<B>> + 'static,
    S::Future: Send + 'static,
    S::Error: Into<Box<dyn std::error::Error + Send + Sync>>,
{
    let mut sig = pin!(Fuse::new(graceful_shutdown_token.cancelled_owned()));

    let mut conn = pin!(builder.serve_connection_with_upgrades(hyper_io, hyper_svc));

    let sleep = sleep_or_pending(max_connection_age);
    tokio::pin!(sleep);

    loop {
        tokio::select! {
            _ = &mut sig => {
                conn.as_mut().graceful_shutdown();
            }
            rv = &mut conn => {
                if let Err(err) = rv {
                    debug!("failed serving connection: {:#}", err);
                }
                break;
            },
            _ = &mut sleep  => {
                conn.as_mut().graceful_shutdown();
                sleep.set(sleep_or_pending(None));
            },
        }
    }

    trace!("connection closed");
    drop(on_connection_close);
}

async fn sleep_or_pending(wait_for: Option<Duration>) {
    match wait_for {
        Some(wait) => tokio::time::sleep(wait).await,
        None => std::future::pending().await,
    };
}

pub(crate) struct OnConnectionClose<A> {
    id: ConnectionId,
    active_connections: ActiveConnections<A>,
}

impl<A> OnConnectionClose<A> {
    pub(crate) fn new(id: ConnectionId, active_connections: ActiveConnections<A>) -> Self {
        Self {
            id,
            active_connections,
        }
    }
}

impl<A> Drop for OnConnectionClose<A> {
    fn drop(&mut self) {
        self.active_connections.write().unwrap().remove(&self.id);
    }
}
