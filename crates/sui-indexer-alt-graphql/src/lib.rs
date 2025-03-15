// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use std::net::SocketAddr;

use anyhow::{self, Context};
use api::query::Query;
use async_graphql::{
    http::GraphiQLSource, EmptyMutation, EmptySubscription, ObjectType, Schema, SchemaBuilder,
    SubscriptionType,
};
use async_graphql_axum::{GraphQLRequest, GraphQLResponse};
use axum::{
    extract::MatchedPath,
    response::Html,
    routing::{get, post},
    Extension, Router,
};
use tokio::{net::TcpListener, task::JoinHandle};
use tokio_util::sync::CancellationToken;
use tracing::{error, info};

mod api;
pub mod args;

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

    /// The GraphQL schema this service will serve.
    schema: SchemaBuilder<Q, M, S>,

    /// Cancellation token controls lifecycle of all RPC-related services.
    cancel: CancellationToken,
}

impl<Q, M, S> RpcService<Q, M, S>
where
    Q: ObjectType + 'static,
    M: ObjectType + 'static,
    S: SubscriptionType + 'static,
{
    pub fn new(args: RpcArgs, schema: SchemaBuilder<Q, M, S>, cancel: CancellationToken) -> Self {
        Self {
            rpc_listen_address: args.rpc_listen_address,
            with_ide: !args.no_ide,
            schema,
            cancel,
        }
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
            schema,
            cancel,
        } = self;

        let schema = schema.finish();
        let mut router: Router = Router::new()
            .route("/graphql", post(graphql::<Q, M, S>))
            .layer(Extension(schema));

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
/// The service may spin up auxiliary services (such as the system package task) to support itself,
/// and will clean these up on shutdown as well.
pub async fn start_rpc(args: RpcArgs, cancel: CancellationToken) -> anyhow::Result<JoinHandle<()>> {
    let h_rpc = RpcService::new(args, schema(), cancel.child_token())
        .run()
        .await?;

    Ok(tokio::spawn(async move {
        let _ = h_rpc.await;
        cancel.cancel();
    }))
}

/// Handler for RPC requests (POST requests making GraphQL queries).
async fn graphql<Q, M, S>(
    Extension(schema): Extension<Schema<Q, M, S>>,
    request: GraphQLRequest,
) -> GraphQLResponse
where
    Q: ObjectType + 'static,
    M: ObjectType + 'static,
    S: SubscriptionType + 'static,
{
    schema.execute(request.into_inner()).await.into()
}

/// Handler for GET requests for the online IDE. GraphQL requests are forwarded to the POST handler
/// at the same path.
async fn graphiql(path: MatchedPath) -> Html<String> {
    Html(GraphiQLSource::build().endpoint(path.as_str()).finish())
}
