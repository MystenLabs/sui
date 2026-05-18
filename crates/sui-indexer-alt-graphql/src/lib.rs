// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use std::any::Any;
use std::net::SocketAddr;
use std::sync::Arc;
use std::time::Duration;

use anyhow::Context as _;
use api::types::address::IAddressable;
use api::types::move_datatype::IMoveDatatype;
use api::types::move_object::IMoveObject;
use api::types::object::IObject;
use async_graphql::ObjectType;
use async_graphql::Schema;
use async_graphql::SchemaBuilder;
use async_graphql::SubscriptionType;
use async_graphql::extensions::ExtensionFactory;
use async_graphql::extensions::Tracing;
use async_graphql_axum::GraphQLRequest;
use async_graphql_axum::GraphQLResponse;
use axum::Extension;
use axum::Router;
use axum::extract::ConnectInfo;
use axum::http::Method;
use axum::response::Html;
use axum::response::IntoResponse;
use axum::routing::MethodRouter;
use axum::routing::get;
use axum::routing::post;
use axum_extra::TypedHeader;
use config::RpcConfig;
use extensions::query_limits::QueryLimitsChecker;
use extensions::query_limits::rich;
use extensions::query_limits::show_usage::ShowUsage;
use extensions::timeout::Timeout;
use futures::StreamExt;
use headers::ContentLength;
use health::DbProbe;
use prometheus::Registry;
use sui_futures::service::Service;
use sui_indexer_alt_reader::consistent_reader::ConsistentReader;
use sui_indexer_alt_reader::consistent_reader::ConsistentReaderArgs;
use sui_indexer_alt_reader::fullnode_client::FullnodeArgs;
use sui_indexer_alt_reader::fullnode_client::FullnodeClient;
use sui_indexer_alt_reader::kv_loader::KvArgs;
use sui_indexer_alt_reader::kv_loader::KvLoader;
use sui_indexer_alt_reader::package_resolver::DbPackageStore;
use sui_indexer_alt_reader::package_resolver::PackageCache;
use sui_indexer_alt_reader::pg_reader::PgReader;
use sui_indexer_alt_reader::pg_reader::db::DbArgs;
use sui_indexer_alt_reader::system_package_task::SystemPackageTask;
use sui_indexer_alt_reader::system_package_task::SystemPackageTaskArgs;
use task::chain_identifier;
use task::watermark::WatermarkTask;
use task::watermark::WatermarksLock;
use tokio::net::TcpListener;
use tokio::sync::oneshot;
use tower_http::catch_panic;
use tower_http::cors;
use tracing::info;
use url::Url;

use crate::api::mutation::Mutation;
use crate::api::query::Query;
#[cfg(feature = "staging")]
use crate::api::subscription::Subscription;
use crate::error::PanicHandler;
use crate::extensions::logging::ClientInfo;
use crate::extensions::logging::Logging;
use crate::extensions::logging::Session;
use crate::metrics::RpcMetrics;
use crate::middleware::version::Version;
#[cfg(not(feature = "staging"))]
use async_graphql::EmptySubscription as Subscription;

const GRAPHQL_PATH: &str = "/graphql";
const GRAPHQL_SUBSCRIPTIONS_PATH: &str = "/graphql/subscriptions";
const HEALTH_PATH: &str = "/graphql/health";

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
        let schema = schema
            .extension(Logging(metrics.clone()))
            .extension(Tracing);

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
            metrics,
        } = self;

        if with_ide {
            info!("Starting GraphiQL IDE at 'http://{rpc_listen_address}{GRAPHQL_PATH}'");
        } else {
            info!("Skipping GraphiQL IDE setup");
        }

        router = router
            .layer(Extension(IdeEnabled(with_ide)))
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
            )
            .layer(catch_panic::CatchPanicLayer::custom(PanicHandler::new(
                metrics,
            )));

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
pub fn schema() -> SchemaBuilder<Query, Mutation, Subscription> {
    Schema::build(Query::default(), Mutation, Subscription)
        .register_output_type::<IAddressable>()
        .register_output_type::<IMoveDatatype>()
        .register_output_type::<IMoveObject>()
        .register_output_type::<IObject>()
}

/// Whether the GraphiQL IDE is enabled on this instance.
#[derive(Clone, Copy)]
struct IdeEnabled(bool);

/// Whether subscriptions are enabled on this instance (i.e., `--checkpoint-stream-url` was set).
#[derive(Clone, Copy)]
struct SubscriptionsEnabled(bool);

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
    kv_args: KvArgs,
    consistent_reader_args: ConsistentReaderArgs,
    args: RpcArgs,
    system_package_task_args: SystemPackageTaskArgs,
    subscription_args: args::SubscriptionArgs,
    version: &'static str,
    config: RpcConfig,
    pg_pipelines: Vec<String>,
    registry: &Registry,
) -> anyhow::Result<Service> {
    let rpc = RpcService::new(args, version, schema(), registry);
    let metrics = rpc.metrics();

    // Create gRPC full node client wrapper. If left unconfigured, the client will not be stored in
    // the schema data, and resolvers that depend on it return `FeatureUnavailable`.
    let fullnode_client = FullnodeClient::new(Some("graphql_fullnode"), fullnode_args, registry)
        .await
        .context("Failed to create fullnode gRPC client")?;

    let consistent_reader =
        ConsistentReader::new(Some("graphql_consistent"), consistent_reader_args, registry).await?;

    let bigtable_reader = kv_args
        .bigtable_reader("indexer-alt-graphql".to_owned(), registry)
        .await?;

    let ledger_grpc_reader = kv_args
        .ledger_grpc_reader(Some("graphql_ledger_grpc"), registry)
        .await?;

    let pg_reader =
        PgReader::new(Some("graphql_db"), database_url.clone(), db_args, registry).await?;

    let pg_loader = Arc::new(pg_reader.as_data_loader());

    let kv_loader = KvLoader::from_kv_sources(
        bigtable_reader.clone(),
        ledger_grpc_reader.clone(),
        pg_loader.clone(),
    );

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
        ledger_grpc_reader.clone(),
        consistent_reader.clone(),
        metrics.clone(),
    );

    let streaming_setup = match subscription_args.checkpoint_stream_url {
        Some(uri) => {
            let ledger_grpc = ledger_grpc_reader
                .clone()
                .context("Ledger gRPC reader is required when streaming is enabled")?;

            let streaming_packages = Arc::new(task::streaming::StreamingPackageStore::new(
                package_store.clone(),
            ));
            // Unbounded is intentional: if `kv_packages` lags long enough for this queue to
            // grow without bound, the indexer infrastructure itself has a bigger problem and
            // OOM on this service is one failure mode among many. Monitor via metrics.
            #[allow(clippy::disallowed_methods)]
            let (package_eviction_tx, package_eviction_rx) = tokio::sync::mpsc::unbounded_channel();
            let readiness =
                task::streaming::SubscriptionReadiness::new(watermark_task.watermarks_rx());
            let stream_task = task::streaming::CheckpointStreamTask::new(
                uri,
                &config.subscription,
                streaming_packages.clone(),
                package_eviction_tx,
                readiness.clone(),
                ledger_grpc,
                watermark_task.watermarks_rx(),
            );
            let eviction_task = task::streaming::PackageEvictionTask::new(
                streaming_packages.clone(),
                package_eviction_rx,
                watermark_task.watermarks(),
                Duration::from_millis(config.subscription.package_eviction_interval_ms),
            );
            Some((stream_task, eviction_task, streaming_packages, readiness))
        }
        None => None,
    };

    let mut rpc = rpc
        .route(GRAPHQL_PATH, post(graphql).get(graphiql))
        .route(GRAPHQL_SUBSCRIPTIONS_PATH, post(graphql_subscriptions))
        .route(HEALTH_PATH, get(health::check))
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
        .data(package_store);

    if let Some(fullnode_client) = fullnode_client {
        rpc = rpc.data(fullnode_client);
    }

    let subscriptions_enabled = streaming_setup.is_some();
    rpc = rpc.layer(SubscriptionsEnabled(subscriptions_enabled));

    let s_system_package_task = system_package_task.run();
    let s_watermark = watermark_task.run();

    // Spawn the streaming tasks and wait for subscriptions to be ready before
    // binding the listener, so the schema is only advertised once `kv_packages`
    // has caught up to the first streamed checkpoint.
    let streaming_handles = if let Some((
        stream_task,
        eviction_task,
        streaming_packages,
        readiness,
    )) = streaming_setup
    {
        rpc = rpc.data(stream_task.broadcaster()).data(streaming_packages);
        let s_stream = stream_task.run();
        let s_eviction = eviction_task.run();
        readiness.wait_for_ready().await?;
        Some((s_stream, s_eviction))
    } else {
        None
    };

    let s_rpc = rpc.run().await?;

    let mut service = s_rpc
        .attach(s_chain_id)
        .attach(s_system_package_task)
        .attach(s_watermark);

    if let Some((s_stream, s_eviction)) = streaming_handles {
        service = service.attach(s_stream).attach(s_eviction);
    }

    Ok(service)
}

/// Handler for RPC requests (POST requests making GraphQL queries).
async fn graphql(
    ConnectInfo(addr): ConnectInfo<SocketAddr>,
    Extension(schema): Extension<Schema<Query, Mutation, Subscription>>,
    Extension(watermark): Extension<WatermarksLock>,
    TypedHeader(content_length): TypedHeader<ContentLength>,
    show_usage: Option<TypedHeader<ShowUsage>>,
    headers: axum::http::HeaderMap,
    request: GraphQLRequest,
) -> GraphQLResponse {
    let mut request = request
        .into_inner()
        .data(content_length)
        .data(Session::new(addr).with_client_info(ClientInfo::from_headers(&headers)))
        .data(watermark.read().await.clone())
        .data(rich::Meter::default());

    if let Some(TypedHeader(show_usage)) = show_usage {
        request = request.data(show_usage);
    }

    schema.execute(request).await.into()
}

/// Handler for GET requests on the GraphQL path. Serves the GraphiQL IDE when enabled,
/// otherwise responds 404. Subscriptions are served separately over SSE at
/// `GRAPHQL_SUBSCRIPTIONS_PATH`.
async fn graphiql(
    Extension(IdeEnabled(ide_enabled)): Extension<IdeEnabled>,
) -> axum::response::Response {
    if !ide_enabled {
        return axum::http::StatusCode::NOT_FOUND.into_response();
    }

    Html(
        include_str!("../assets/graphiql.html")
            .replace("__GRAPHQL_PATH__", GRAPHQL_PATH)
            .replace("__GRAPHQL_SUBSCRIPTIONS_PATH__", GRAPHQL_SUBSCRIPTIONS_PATH),
    )
    .into_response()
}

async fn graphql_subscriptions(
    ConnectInfo(addr): ConnectInfo<SocketAddr>,
    Extension(schema): Extension<Schema<Query, Mutation, Subscription>>,
    Extension(SubscriptionsEnabled(subscriptions_enabled)): Extension<SubscriptionsEnabled>,
    Extension(watermark): Extension<WatermarksLock>,
    request: GraphQLRequest,
) -> axum::response::Response {
    if !subscriptions_enabled {
        return (
            axum::http::StatusCode::NOT_FOUND,
            "Subscriptions are not enabled on this instance.",
        )
            .into_response();
    }

    let watermarks = watermark.read().await.clone();
    let req = request
        .into_inner()
        .data(Session::new(addr))
        .data(watermarks)
        .data(rich::Meter::default());

    let stream = schema.execute_stream(req).map(|response| {
        let payload = serde_json::to_string(&response).unwrap_or_else(|_| "null".into());
        Ok::<_, std::convert::Infallible>(
            axum::response::sse::Event::default()
                .event("next")
                .data(payload),
        )
    });

    axum::response::sse::Sse::new(stream)
        .keep_alive(
            axum::response::sse::KeepAlive::default()
                .interval(Duration::from_secs(15))
                .text("keep-alive"),
        )
        .into_response()
}

#[cfg(test)]
mod tests {
    use std::fs;
    use std::net::IpAddr;
    use std::net::Ipv4Addr;
    use std::path::PathBuf;

    use async_graphql::EmptyMutation;
    use async_graphql::EmptySubscription;
    use async_graphql::Object;
    use async_graphql::SDLExportOptions;
    use async_graphql::Schema;
    use async_graphql_axum::GraphQLRequest;
    use async_graphql_axum::GraphQLResponse;
    use axum::routing::post;
    use insta::assert_snapshot;
    use reqwest::Client;
    use serde_json::Value;
    use serde_json::json;
    use sui_pg_db::temp::get_available_port;

    use crate::error::code;
    use crate::extensions::logging::Session;

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

    #[tokio::test]
    async fn test_panic_handling() {
        struct Query;

        #[Object]
        impl Query {
            async fn panic(&self) -> bool {
                assert_eq!(1, 2, "Boom!");
                true
            }
        }

        async fn graphql(
            Extension(schema): Extension<Schema<Query, EmptyMutation, EmptySubscription>>,
            request: GraphQLRequest,
        ) -> GraphQLResponse {
            let request = request
                .into_inner()
                .data(Session::new("0.0.0.0:0".parse().unwrap()));
            schema.execute(request).await.into()
        }

        let registry = Registry::new();
        let rpc_listen_address =
            SocketAddr::new(IpAddr::V4(Ipv4Addr::LOCALHOST), get_available_port());

        let rpc = RpcService::new(
            RpcArgs {
                rpc_listen_address,
                no_ide: true,
            },
            "test",
            Schema::build(Query, EmptyMutation, EmptySubscription),
            &registry,
        )
        .route("/graphql", post(graphql));

        let metrics = rpc.metrics();
        let _svc = rpc.run().await.unwrap();

        let url = format!("http://{rpc_listen_address}/graphql");
        let client = Client::new();

        let resp = client
            .post(&url)
            .json(&json!({
                "query": "{ panic }"
            }))
            .send()
            .await
            .expect("Request should succeed");

        assert_eq!(resp.status(), 500);

        let body: Value = resp.json().await.expect("Response should be JSON");

        // Verify the response is a GraphQL error
        let error = &body["errors"].as_array().unwrap()[0];

        assert!(
            error["message"]
                .as_str()
                .unwrap()
                .contains("Request panicked")
        );

        assert_eq!(
            error["extensions"]["code"].as_str(),
            Some(code::INTERNAL_SERVER_ERROR)
        );

        // The panic message is in the chain
        let chain = error["extensions"]["chain"].as_array().unwrap();
        assert!(chain.iter().any(|c| c.as_str().unwrap().contains("Boom!")));

        // Verify the panic is recorded in metrics
        assert_eq!(metrics.queries_panicked.get(), 1);
    }
}
