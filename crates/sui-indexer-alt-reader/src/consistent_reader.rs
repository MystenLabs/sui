// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use std::future::Future;
use std::sync::Arc;
use std::time::Duration;

use anyhow::{anyhow, Context};
use prometheus::Registry;
use sui_indexer_alt_consistent_api::proto::rpc::consistent::v1alpha::{
    consistent_service_client::ConsistentServiceClient, owner::OwnerKind, AvailableRangeRequest,
    AvailableRangeResponse, End, ListObjectsByTypeRequest, ListOwnedObjectsRequest, Object, Owner,
    CHECKPOINT_METADATA,
};
use sui_types::base_types::{ObjectDigest, ObjectID, ObjectRef, SequenceNumber};
use tokio_util::sync::CancellationToken;
use tonic::transport::Channel;
use tracing::instrument;
use url::Url;

pub use sui_indexer_alt_consistent_api::proto::rpc::consistent::v1alpha as proto;

use crate::metrics::ConsistentReaderMetrics;

/// Like `anyhow::bail!`, but returns this module's `Error` type, not `anyhow::Error`.
macro_rules! bail {
    ($e:expr) => {
        return Err(Error::Internal(anyhow!($e)));
    };
}

#[derive(clap::Args, Debug, Clone, Default)]
pub struct ConsistentReaderArgs {
    /// URL of the consistent store gRPC service
    #[arg(long)]
    pub consistent_store_url: Option<Url>,

    /// Time spent waiting for a request to complete, in milliseconds
    #[arg(long)]
    pub consistent_store_statement_timeout_ms: Option<u64>,
}

/// A reader backed by the consistent store gRPC service.
#[derive(Clone)]
pub struct ConsistentReader {
    client: Option<Client>,
    timeout: Option<Duration>,
    metrics: Arc<ConsistentReaderMetrics>,
    cancel: CancellationToken,
}

/// Response from a paginated query.
pub struct Page<T> {
    pub results: Vec<Edge<T>>,
    pub has_previous_page: bool,
    pub has_next_page: bool,
}

/// An individual element from a paginated query, includes the value and its token (pagination
/// cursor).
pub struct Edge<T> {
    pub token: Vec<u8>,
    pub value: T,
}

type Client = ConsistentServiceClient<Channel>;

#[derive(thiserror::Error, Debug)]
pub enum Error {
    #[error(transparent)]
    Internal(#[from] anyhow::Error),

    #[error("{}", .0.message())]
    OutOfRange(#[source] tonic::Status),

    #[error("Consistent store client not configured")]
    NotConfigured,
}

impl ConsistentReaderArgs {
    pub fn statement_timeout(&self) -> Option<Duration> {
        self.consistent_store_statement_timeout_ms
            .map(Duration::from_millis)
    }
}

impl ConsistentReader {
    pub async fn new(
        prefix: Option<&str>,
        args: ConsistentReaderArgs,
        registry: &Registry,
        cancel: CancellationToken,
    ) -> Result<Self, Error> {
        let client = if let Some(url) = &args.consistent_store_url {
            let mut endpoint = Channel::from_shared(url.to_string())
                .context("Failed to create channel for gRPC endpoint")?;

            if let Some(timeout) = args.statement_timeout() {
                endpoint = endpoint.timeout(timeout);
            }

            let channel = endpoint
                .connect()
                .await
                .context("Failed to connect to gRPC endpoint")?;

            Some(ConsistentServiceClient::new(channel))
        } else {
            None
        };

        let timeout = args.statement_timeout();
        let metrics = ConsistentReaderMetrics::new(prefix, registry);

        Ok(Self {
            client,
            timeout,
            metrics,
            cancel,
        })
    }

    /// Get the consistent store's watermarks, as of the given `checkpoint`.
    #[instrument(skip(self), level = "debug")]
    pub async fn available_range(&self, checkpoint: u64) -> Result<AvailableRangeResponse, Error> {
        self.request(
            "available_range",
            Some(checkpoint),
            |mut client, request| async move { client.available_range(request).await },
            AvailableRangeRequest {},
        )
        .await
    }

    /// Paginate live objects with type filter `object_type`, at checkpoint `checkpoint`.
    #[instrument(skip(self), level = "debug")]
    pub async fn list_objects_by_type(
        &self,
        checkpoint: u64,
        object_type: String,
        page_size: Option<u32>,
        after_token: Option<Vec<u8>>,
        before_token: Option<Vec<u8>>,
        is_from_front: bool,
    ) -> Result<Page<ObjectRef>, Error> {
        let response = self
            .request(
                "list_objects_by_type",
                Some(checkpoint),
                |mut client, request| async move { client.list_objects_by_type(request).await },
                ListObjectsByTypeRequest {
                    object_type: Some(object_type),
                    page_size,
                    after_token: after_token.map(Into::into),
                    before_token: before_token.map(Into::into),
                    end: if is_from_front {
                        Some(End::Front.into())
                    } else {
                        Some(End::Back.into())
                    },
                },
            )
            .await?;

        let has_next_page = response.has_next_page();
        let has_previous_page = response.has_previous_page();

        let results = response
            .objects
            .into_iter()
            .map(TryFrom::try_from)
            .collect::<Result<Vec<_>, _>>()?;

        Ok(Page {
            results,
            has_next_page,
            has_previous_page,
        })
    }

    /// Paginate live objects at `checkpoint`, with owner described by `kind` and `address`, and an
    /// optional `object_type` filter.
    #[instrument(skip(self), level = "debug")]
    pub async fn list_owned_objects(
        &self,
        checkpoint: u64,
        kind: OwnerKind,
        address: Option<String>,
        object_type: Option<String>,
        page_size: Option<u32>,
        after_token: Option<Vec<u8>>,
        before_token: Option<Vec<u8>>,
        is_from_front: bool,
    ) -> Result<Page<ObjectRef>, Error> {
        let response = self
            .request(
                "list_owned_objects",
                Some(checkpoint),
                |mut client, request| async move { client.list_owned_objects(request).await },
                ListOwnedObjectsRequest {
                    owner: Some(Owner {
                        kind: Some(kind.into()),
                        address,
                    }),
                    object_type,
                    page_size,
                    after_token: after_token.map(Into::into),
                    before_token: before_token.map(Into::into),
                    end: if is_from_front {
                        Some(End::Front.into())
                    } else {
                        Some(End::Back.into())
                    },
                },
            )
            .await?;

        let has_next_page = response.has_next_page();
        let has_previous_page = response.has_previous_page();

        let results = response
            .objects
            .into_iter()
            .map(TryFrom::try_from)
            .collect::<Result<Vec<_>, _>>()?;

        Ok(Page {
            results,
            has_next_page,
            has_previous_page,
        })
    }

    async fn request<I, O, Fut, F>(
        &self,
        method: &str,
        checkpoint: Option<u64>,
        response: F,
        input: I,
    ) -> Result<O, Error>
    where
        F: FnOnce(Client, tonic::Request<I>) -> Fut,
        Fut: Future<Output = Result<tonic::Response<O>, tonic::Status>>,
    {
        let Some(client) = self.client.clone() else {
            return Err(Error::NotConfigured);
        };

        self.metrics
            .requests_received
            .with_label_values(&[method])
            .inc();

        let _timer = self
            .metrics
            .latency
            .with_label_values(&[method])
            .start_timer();

        let mut request = tonic::Request::new(input);

        if let Some(timeout) = self.timeout {
            request.set_timeout(timeout);
        }

        if let Some(checkpoint) = checkpoint {
            request.metadata_mut().insert(
                CHECKPOINT_METADATA,
                checkpoint
                    .to_string()
                    .parse()
                    .with_context(|| format!("Invalid checkpoint {checkpoint}"))?,
            );
        }

        let response = tokio::select! {
            _ = self.cancel.cancelled() => {
                bail!("Request cancelled");
            }

            r = response(client, request) => {
                r.map(|r| r.into_inner()).map_err(Into::into)
            }
        };

        if response.is_ok() {
            self.metrics
                .requests_succeeded
                .with_label_values(&[method])
                .inc();
        } else {
            self.metrics
                .requests_failed
                .with_label_values(&[method])
                .inc()
        }

        response
    }
}

impl TryFrom<Object> for Edge<ObjectRef> {
    type Error = Error;

    fn try_from(proto: Object) -> Result<Self, Error> {
        let object_id: ObjectID = proto
            .object_id
            .context("object ID missing")?
            .parse()
            .context("invalid object ID")?;

        let digest: ObjectDigest = proto
            .digest
            .context("digest missing")?
            .parse()
            .context("invalid digest")?;

        let version: SequenceNumber = proto.version.context("version missing")?.into();
        let token: Vec<u8> = proto.page_token.unwrap_or_default().into();

        Ok(Edge {
            token,
            value: (object_id, version, digest),
        })
    }
}

impl From<tonic::Status> for Error {
    fn from(status: tonic::Status) -> Self {
        match status.code() {
            tonic::Code::OutOfRange => Error::OutOfRange(status),
            _ => Error::Internal(anyhow!(status.code()).context(status.message().to_string())),
        }
    }
}
