// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use std::future::Future;
use std::time::Duration;

use futures::TryStreamExt;
use tokio::time::Instant;
use tonic::codegen::BoxStream;

use crate::RpcError;
use crate::grpc::deadline::with_deadline;

pub(super) async fn serve_list_stream<T, Fut>(
    operation: &'static str,
    timeout: Duration,
    stream: Fut,
) -> Result<tonic::Response<BoxStream<T>>, tonic::Status>
where
    T: Send + 'static,
    Fut: Future<Output = Result<futures::stream::BoxStream<'static, Result<T, RpcError>>, RpcError>>
        + Send,
{
    let deadline = Instant::now() + timeout;

    let stream = tokio::time::timeout_at(deadline, stream)
        .await
        .map_err(|_| {
            tracing::warn!(operation, "construction phase timed out");
            tonic::Status::deadline_exceeded(format!("{operation} request deadline exceeded"))
        })?
        .map_err(tonic::Status::from)?
        .map_err(tonic::Status::from);

    Ok(tonic::Response::new(with_deadline(
        stream, deadline, operation,
    )))
}
