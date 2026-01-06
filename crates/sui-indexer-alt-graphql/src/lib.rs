// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use std::{any::Any, net::SocketAddr, sync::Arc};

use anyhow::{self, Context};
use api::types::{
    address::IAddressable, move_datatype::IMoveDatatype, move_object::IMoveObject, object::IObject,
};
use async_graphql::{
    EmptySubscription, ObjectType, Schema, SchemaBuilder, SubscriptionType,
    extensions::ExtensionFactory, http::GraphiQLSource,
};
use async_graphql_axum::{GraphQLRequest, GraphQLResponse};
use axum::{
    Extension, Router,
    extract::{ConnectInfo, MatchedPath},
    http::Method,
    response::Html,
    routing::{MethodRouter, get, post},
};
use axum_extra::TypedHeader;
use config::RpcConfig;
use extensions::{
    query_limits::{QueryLimitsChecker, show_usage::ShowUsage},
    timeout::Timeout,
};
use headers::ContentLength;
use health::DbProbe;
use prometheus::Registry;
use sui_futures::service::Service;
use sui_indexer_alt_reader::pg_reader::db::DbArgs;
use sui_indexer_alt_reader::system_package_task::{SystemPackageTask, SystemPackageTaskArgs};
use sui_indexer_alt_reader::{
    bigtable_reader::BigtableReader,
    consistent_reader::{ConsistentReader, ConsistentReaderArgs},
    fullnode_client::{FullnodeArgs, FullnodeClient},
    kv_loader::KvLoader,
    ledger_grpc_reader::LedgerGrpcReader,
    package_resolver::{DbPackageStore, PackageCache},
    pg_reader::PgReader,
};
use task::{
    chain_identifier,
    watermark::{WatermarkTask, WatermarksLock},
};
use tokio::{net::TcpListener, sync::oneshot};
use tower_http::cors;
use tracing::info;
use url::Url;

use crate::api::{mutation::Mutation, query::Query};
use crate::extensions::logging::{Logging, Session};
use crate::metrics::RpcMetrics;
use crate::middleware::version::Version;

mod api;
pub mod args;
pub mod config;
mod error;
pub mod extensions;
mod health;
mod intersect;
mod metrics;
mod middleware;
mod pagination;
mod scope;
mod task;

#[derive(clap::Args, Clone, Debug)]
pub struct RpcArgs {
    /// Address to accept incoming RPC connections on.
    #[clap(long, default_value_t = Self::default().rpc_listen_address)]
    pub rpc_listen_address: SocketAddr,

    /// Do not expose the GraphiQL IDE.
    #[clap(long, default_value_t = Self::default().no_ide)]
    pub no_ide: bool,
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

    /// The axum router that will handle incoming requests.
    router: Router,

    /// The GraphQL schema this service will serve.
    schema: SchemaBuilder<Q, M, S>,

    /// Metrics for the RPC service.
    metrics: Arc<RpcMetrics>,
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
    ) -> Self {
        let RpcArgs {
            rpc_listen_address,
            no_ide,
        } = args;

        let metrics = RpcMetrics::new(registry);
        let router = Router::new();

        // The logging extension should be outermost so that it can surround all other extensions.
        let schema = schema.extension(Logging(metrics.clone()));

        Self {
            rpc_listen_address,
            with_ide: !no_ide,
            version,
            router,
            schema,
            metrics,
        }
    }

    /// Return a copy of the metrics.
    pub fn metrics(&self) -> Arc<RpcMetrics> {
        self.metrics.clone()
    }

    /// Add a handler to the axum router.
    pub fn route(self, path: &str, router: MethodRouter) -> Self {
        let router = self.router.route(path, router);
        Self { router, ..self }
    }

    /// Add an extension as a layer to the axum router.
    pub fn layer<T>(self, layer: T) -> Self
    where
        T: Clone + Send + Sync + 'static,
    {
        let router = self.router.layer(Extension(layer));
        Self { router, ..self }
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
    pub async fn run(self) -> anyhow::Result<Service>
    where
        Q: ObjectType + 'static,
        M: ObjectType + 'static,
        S: SubscriptionType + 'static,
    {
        let Self {
            rpc_listen_address,
            with_ide,
            version,
            mut router,
            schema,
            metrics: _,
        } = self;

        if with_ide {
            info!("Starting GraphiQL IDE at 'http://{rpc_listen_address}/graphql'");
            router = router.route("/graphql", get(graphiql));
        } else {
            info!("Skipping GraphiQL IDE setup");
        }

        router = router
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

        info!("Starting GraphQL service on {rpc_listen_address}");
        let listener = TcpListener::bind(rpc_listen_address)
            .await
            .context("Failed to bind GraphQL to listen address")?;

        let (stx, srx) = oneshot::channel::<()>();
        Ok(Service::new()
            .with_shutdown_signal(async move {
                let _ = stx.send(());
            })
            .spawn(async move {
                axum::serve(
                    listener,
                    router.into_make_service_with_connect_info::<SocketAddr>(),
                )
                .with_graceful_shutdown(async move {
                    let _ = srx.await;
                    info!("Shutdown received, shutting down GraphQL service");
                })
                .await
                .context("Failed to start GraphQL service")
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
pub fn schema() -> SchemaBuilder<Query, Mutation, EmptySubscription> {
    Schema::build(Query::default(), Mutation, EmptySubscription)
        .register_output_type::<IAddressable>()
        .register_output_type::<IMoveDatatype>()
        .register_output_type::<IMoveObject>()
        .register_output_type::<IObject>()
}

/// Set-up and run the RPC service, using the provided arguments (expected to be extracted from the
/// command-line).
///
/// Access to most reads is controlled by the `database_url` -- if it is `None`, those reads will
/// not work. KV queries can optionally be served by a Bigtable instance or Ledger gRPC service
/// via `kv_args`. If a Bigtable instance is configured, the `GOOGLE_APPLICATION_CREDENTIALS`
/// environment variable must point to the credentials JSON file.
///
/// `version` is the version string reported in response headers by the service as part of every
/// request.
///
/// The service may spin up auxiliary services (such as the system package task) to support itself,
/// and will clean these up on shutdown as well.
pub async fn start_rpc(
    database_url: Option<Url>,
    fullnode_args: FullnodeArgs,
    db_args: DbArgs,
    kv_args: args::KvArgs,
    consistent_reader_args: ConsistentReaderArgs,
    args: RpcArgs,
    system_package_task_args: SystemPackageTaskArgs,
    version: &'static str,
    config: RpcConfig,
    pg_pipelines: Vec<String>,
    registry: &Registry,
) -> anyhow::Result<Service> {
    let rpc = RpcService::new(args, version, schema(), registry);
    let metrics = rpc.metrics();

    // Create gRPC full node client wrapper
    let fullnode_client =
        FullnodeClient::new(Some("graphql_fullnode"), fullnode_args, registry).await?;

    let pg_reader =
        PgReader::new(Some("graphql_db"), database_url.clone(), db_args, registry).await?;

    let bigtable_reader = if let Some(instance_id) = kv_args.bigtable_instance.as_ref() {
        let reader = BigtableReader::new(
            instance_id.clone(),
            "indexer-alt-graphql".to_owned(),
            kv_args.bigtable_args(),
            registry,
        )
        .await?;

        Some(reader)
    } else {
        None
    };

    let ledger_grpc_reader = if let Some(ledger_grpc_url) = kv_args.ledger_grpc_url.as_ref() {
        let reader = LedgerGrpcReader::new(ledger_grpc_url.clone(), kv_args.ledger_grpc_args())
            .await
            .context("Failed to create Ledger gRPC reader")?;
        Some(reader)
    } else {
        None
    };

    let consistent_reader =
        ConsistentReader::new(Some("graphql_consistent"), consistent_reader_args, registry).await?;

    let pg_loader = Arc::new(pg_reader.as_data_loader());
    let kv_loader = if let Some(reader) = bigtable_reader.as_ref() {
        KvLoader::new_with_bigtable(Arc::new(reader.as_data_loader()))
    } else if let Some(reader) = ledger_grpc_reader.as_ref() {
        KvLoader::new_with_ledger_grpc(Arc::new(reader.as_data_loader()))
    } else {
        KvLoader::new_with_pg(pg_loader.clone())
    };

    let package_store = Arc::new(PackageCache::new(DbPackageStore::new(pg_loader.clone())));

    let system_package_task = SystemPackageTask::new(
        system_package_task_args,
        pg_reader.clone(),
        package_store.clone(),
    );

    // Fetch and cache the chain identifier from the database.
    let (chain_identifier, s_chain_id) = chain_identifier::task(
        pg_reader.clone(),
        config.watermark.watermark_polling_interval,
    );

    let watermark_task = WatermarkTask::new(
        config.watermark,
        pg_pipelines,
        pg_reader.clone(),
        bigtable_reader,
        ledger_grpc_reader,
        consistent_reader.clone(),
        metrics.clone(),
    );

    let rpc = rpc
        .route("/graphql", post(graphql))
        .route("/graphql/health", get(health::check))
        .layer(watermark_task.watermarks())
        .layer(config.health)
        .layer(DbProbe(database_url))
        .extension(Timeout::new(config.limits.timeouts()))
        .extension(QueryLimitsChecker::new(
            config.limits.query_limits(),
            metrics,
        ))
        .data(config.limits.pagination())
        .data(config.limits)
        .data(config.name_service)
        .data(config.zklogin)
        .data(chain_identifier)
        .data(pg_reader)
        .data(consistent_reader)
        .data(pg_loader)
        .data(kv_loader)
        .data(package_store)
        .data(fullnode_client);

    let s_rpc = rpc.run().await?;
    let s_system_package_task = system_package_task.run();
    let s_watermark = watermark_task.run();

    Ok(s_rpc
        .attach(s_chain_id)
        .attach(s_system_package_task)
        .attach(s_watermark))
}

/// Handler for RPC requests (POST requests making GraphQL queries).
async fn graphql(
    ConnectInfo(addr): ConnectInfo<SocketAddr>,
    Extension(schema): Extension<Schema<Query, Mutation, EmptySubscription>>,
    Extension(watermark): Extension<WatermarksLock>,
    TypedHeader(content_length): TypedHeader<ContentLength>,
    show_usage: Option<TypedHeader<ShowUsage>>,
    request: GraphQLRequest,
) -> GraphQLResponse {
    let mut request = request
        .into_inner()
        .data(content_length)
        .data(Session::new(addr))
        .data(watermark.read().await.clone());

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
    use async_graphql::SDLExportOptions;
    use insta::assert_snapshot;
    use std::fs;
    use std::path::PathBuf;

    use super::*;

    /// Check that the exported schema is up-to-date.
    #[test]
    fn test_schema_sdl_export() {
        let options = SDLExportOptions::new().sorted_fields();
        let sdl = schema().finish().sdl_with_options(options);

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
