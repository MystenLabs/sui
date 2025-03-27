// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use std::{any::Any, net::SocketAddr, sync::Arc};

use anyhow::{self, Context};
use async_graphql::{
    extensions::ExtensionFactory, http::GraphiQLSource, EmptyMutation, EmptySubscription,
    ObjectType, Schema, SchemaBuilder, SubscriptionType,
};
use async_graphql_axum::{GraphQLRequest, GraphQLResponse};
use axum::{
    extract::{ConnectInfo, MatchedPath},
    http::Method,
    response::Html,
    routing::{get, post},
    Extension, Router,
};
use axum_extra::TypedHeader;
use config::RpcConfig;
use extensions::{
    query_limits::{show_usage::ShowUsage, QueryLimitsChecker},
    timeout::Timeout,
};
use headers::ContentLength;
use prometheus::Registry;
use tokio::{net::TcpListener, task::JoinHandle};
use tokio_util::sync::CancellationToken;
use tower_http::cors;
use tracing::{error, info};

use crate::api::query::Query;
use crate::extensions::logging::{Logging, Session};
use crate::metrics::RpcMetrics;
use crate::middleware::version::Version;

mod api;
pub mod args;
pub mod config;
mod error;
mod extensions;
mod metrics;
mod middleware;
mod pagination;

#[derive(clap::Args, Clone, Debug)]
pub struct RpcArgs {
    /// Address to accept incoming RPC connections on.
    #[clap(long, default_value_t = Self::default().rpc_listen_address)]
    rpc_listen_address: SocketAddr,

    /// Do not expose the GraphiQL IDE.
    #[clap(long, default_value_t = Self::default().no_ide)]
    no_ide: bool,
}

/// This type is responsible for the set-up and lifecycle of all services related to a GraphQL RPC
/// service (the RPC service, online IDE, health checks etc). It is agnostic to the schema
/// being served (which must be provided to `run`).
pub struct RpcService<Q, M, S> {
    /// Address to accept incoming RPC connections on.
    rpc_listen_address: SocketAddr,

    /// Whether to expose GraphiQL IDE on GET requests
    with_ide: bool,

    /// The version string to report with each response, as an HTTP header.
    version: &'static str,

    /// The GraphQL schema this service will serve.
    schema: SchemaBuilder<Q, M, S>,

    /// Metrics for the RPC service.
    metrics: Arc<RpcMetrics>,

    /// Cancellation token controls lifecycle of all RPC-related services.
    cancel: CancellationToken,
}

impl<Q, M, S> RpcService<Q, M, S>
where
    Q: ObjectType + 'static,
    M: ObjectType + 'static,
    S: SubscriptionType + 'static,
{
    pub fn new(
        args: RpcArgs,
        version: &'static str,
        schema: SchemaBuilder<Q, M, S>,
        registry: &Registry,
        cancel: CancellationToken,
    ) -> Self {
        let RpcArgs {
            rpc_listen_address,
            no_ide,
        } = args;

        let metrics = RpcMetrics::new(registry);

        // The logging extension should be outermost so that it can surround all other extensions.
        let schema = schema.extension(Logging(metrics.clone()));

        Self {
            rpc_listen_address,
            with_ide: !no_ide,
            version,
            schema,
            metrics,
            cancel,
        }
    }

    /// Return a copy of the metrics.
    pub fn metrics(&self) -> Arc<RpcMetrics> {
        self.metrics.clone()
    }

    /// Add an extension to the GraphQL schema.
    pub fn extension(self, ext: impl ExtensionFactory) -> Self {
        let schema = self.schema.extension(ext);
        Self { schema, ..self }
    }

    /// Add data to the GraphQL schema that can be accessed from the Context from any request.
    pub fn data<D: Any + Send + Sync>(self, data: D) -> Self {
        let schema = self.schema.data(data);
        Self { schema, ..self }
    }

    /// Run the RPC service. This binds the listener and exposes handlers for the RPC service and IDE
    /// (if enabled).
    pub async fn run(self) -> anyhow::Result<JoinHandle<()>>
    where
        Q: ObjectType + 'static,
        M: ObjectType + 'static,
        S: SubscriptionType + 'static,
    {
        let Self {
            rpc_listen_address,
            with_ide,
            version,
            schema,
            metrics: _,
            cancel,
        } = self;

        let mut router: Router = Router::new()
            .route("/graphql", post(graphql::<Q, M, S>))
            .layer(Extension(schema.finish()))
            .layer(axum::middleware::from_fn_with_state(
                Version(version),
                middleware::version::set_version,
            ))
            .layer(
                cors::CorsLayer::new()
                    .allow_methods([Method::POST])
                    .allow_origin(cors::Any)
                    .allow_headers(cors::Any),
            );

        if with_ide {
            info!("Starting GraphiQL IDE at 'http://{rpc_listen_address}/graphql'");
            router = router.route("/graphql", get(graphiql));
        } else {
            info!("Skipping GraphiQL IDE setup");
        }

        info!("Starting GraphQL service on {rpc_listen_address}");
        let listener = TcpListener::bind(rpc_listen_address)
            .await
            .context("Failed to bind GraphQL to listen address")?;

        let service = axum::serve(
            listener,
            router.into_make_service_with_connect_info::<SocketAddr>(),
        )
        .with_graceful_shutdown({
            let cancel = cancel.clone();
            async move {
                cancel.cancelled().await;
                info!("Shutdown received, shutting down GraphQL service");
            }
        });

        Ok(tokio::spawn(async move {
            if let Err(e) = service.await.context("Failed to start GraphQL service") {
                error!("Failed to start GraphQL service: {e:?}");
                cancel.cancel();
            }
        }))
    }
}

impl Default for RpcArgs {
    fn default() -> Self {
        Self {
            rpc_listen_address: "0.0.0.0:7000".parse().unwrap(),
            no_ide: false,
        }
    }
}

/// The GraphQL schema this service will serve, without any extensions or context added.
pub fn schema() -> SchemaBuilder<Query, EmptyMutation, EmptySubscription> {
    Schema::build(Query, EmptyMutation, EmptySubscription)
}

/// Set-up and run the RPC service, using the provided arguments (expected to be extracted from the
/// command-line). The service will continue to run until the cancellation token is triggered, and
/// will signal cancellation on the token when it is shutting down.
///
/// `version` is the version string reported in response headers by the service as part of every
/// request.
///
/// The service may spin up auxiliary services (such as the system package task) to support itself,
/// and will clean these up on shutdown as well.
pub async fn start_rpc(
    args: RpcArgs,
    version: &'static str,
    config: RpcConfig,
    registry: &Registry,
    cancel: CancellationToken,
) -> anyhow::Result<JoinHandle<()>> {
    let rpc = RpcService::new(args, version, schema(), registry, cancel.child_token());
    let metrics = rpc.metrics();

    let rpc = rpc
        .extension(Timeout::new(config.limits.timeouts()))
        .extension(QueryLimitsChecker::new(
            config.limits.query_limits(),
            metrics,
        ))
        .data(config.limits.pagination())
        .data(config.limits);

    let h_rpc = rpc.run().await?;

    Ok(tokio::spawn(async move {
        let _ = h_rpc.await;
        cancel.cancel();
    }))
}

/// Handler for RPC requests (POST requests making GraphQL queries).
async fn graphql<Q, M, S>(
    ConnectInfo(addr): ConnectInfo<SocketAddr>,
    Extension(schema): Extension<Schema<Q, M, S>>,
    TypedHeader(content_length): TypedHeader<ContentLength>,
    show_usage: Option<TypedHeader<ShowUsage>>,
    request: GraphQLRequest,
) -> GraphQLResponse
where
    Q: ObjectType + 'static,
    M: ObjectType + 'static,
    S: SubscriptionType + 'static,
{
    let mut request = request
        .into_inner()
        .data(content_length)
        .data(Session::new(addr));

    if let Some(TypedHeader(show_usage)) = show_usage {
        request = request.data(show_usage);
    }

    schema.execute(request).await.into()
}

/// Handler for GET requests for the online IDE. GraphQL requests are forwarded to the POST handler
/// at the same path.
async fn graphiql(path: MatchedPath) -> Html<String> {
    Html(GraphiQLSource::build().endpoint(path.as_str()).finish())
}

#[cfg(test)]
mod tests {
    use std::fs;
    use std::path::PathBuf;

    use insta::assert_snapshot;

    use super::*;

    /// Check that the exported schema is up-to-date.
    #[test]
    fn test_schema_sdl_export() {
        let sdl = schema().finish().sdl();

        let file = if cfg!(feature = "staging") {
            "staging.graphql"
        } else {
            "schema.graphql"
        };

        // Update the current schema file
        let path = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join(file);
        fs::write(path, &sdl).unwrap();

        assert_snapshot!(file, sdl);
    }
}
