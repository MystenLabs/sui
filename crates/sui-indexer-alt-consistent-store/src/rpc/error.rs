// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0
#![allow(dead_code)]

use std::{convert::Infallible, str::FromStr, sync::Arc};

use tonic::{
    metadata::{MetadataMap, MetadataValue},
    Status,
};

pub(super) trait StatusCode {
    fn code(&self) -> tonic::Code;
}

#[derive(thiserror::Error, Debug, Clone)]
pub(super) enum RpcError<E = Infallible> {
    Unimplemented,

    /// Checkpoint requested is not in the available range.
    NotInRange(u64),

    /// A custom error type to cover the service or method-specific error cases.
    Custom(Arc<E>),

    /// Error to wrap existing framework errors.
    Status(#[from] tonic::Status),

    /// An error produced by the internal works of the service (our fault).
    InternalError(Arc<anyhow::Error>),
}

impl StatusCode for Infallible {
    fn code(&self) -> tonic::Code {
        match *self {}
    }
}

impl<E: std::error::Error + StatusCode> From<E> for RpcError<E> {
    fn from(err: E) -> Self {
        RpcError::Custom(Arc::new(err))
    }
}

/// Cannot use `#[from]` for this conversion because [`anyhow::Error`] does not implement `Clone`,
/// so it needs to be wrapped in an [`Arc`].
impl<E> From<anyhow::Error> for RpcError<E> {
    fn from(err: anyhow::Error) -> Self {
        RpcError::InternalError(Arc::new(err))
    }
}

impl<E> From<RpcError<E>> for Status
where
    E: std::error::Error + StatusCode,
    E: Send + Sync + 'static,
{
    fn from(err: RpcError<E>) -> Self {
        match err {
            RpcError::Unimplemented => Status::unimplemented("Not implemented yet"),

            RpcError::NotInRange(checkpoint) => Status::out_of_range(format!(
                "Checkpoint {checkpoint} not in the consistent range"
            )),

            RpcError::Custom(err) => {
                let mut status = Status::new(err.code(), err.to_string());
                status.set_source(err);
                status
            }

            RpcError::Status(status) => status,

            RpcError::InternalError(err) => {
                let mut chain = err.chain().enumerate();
                let Some((_, top)) = chain.next() else {
                    return Status::internal("Unknown error");
                };

                let mut meta = MetadataMap::new();
                for (ix, err) in chain {
                    let data = MetadataValue::from_str(&format!("{ix}: {err}"))
                        .unwrap_or_else(|_| MetadataValue::from_static("[invalid]"));
                    meta.append("x-sui-rpc-chain", data);
                }

                Status::with_metadata(tonic::Code::Internal, top.to_string(), meta)
            }
        }
    }
}
