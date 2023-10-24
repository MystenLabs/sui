// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use crate::{
    config::ServerConfig,
    context_data::{
        data_provider::DataProvider,
        db_data_provider::PgManager,
        sui_sdk_data_provider::{lru_cache_data_loader, sui_sdk_client_v0},
    },
    error::Error,
    extensions::{
        feature_gate::FeatureGate,
        logger::Logger,
        query_limits_checker::{QueryLimitsChecker, ShowUsage},
        timeout::Timeout,
    },
    metrics::RequestMetrics,
    server::version::{check_version_middleware, set_version_middleware},
    types::query::{Query, SuiGraphQLSchema},
};
use async_graphql::{extensions::ExtensionFactory, Schema, SchemaBuilder};
use async_graphql::{EmptyMutation, EmptySubscription};
use async_graphql_axum::{GraphQLRequest, GraphQLResponse};
use axum::http::HeaderMap;
use axum::{
    extract::{connect_info::IntoMakeServiceWithConnectInfo, ConnectInfo},
    middleware,
};
use axum::{headers::Header, Router};
use hyper::server::conn::AddrIncoming as HyperAddrIncoming;
use hyper::Server as HyperServer;
use std::{any::Any, net::SocketAddr, sync::Arc};

pub struct Server {
    pub server: HyperServer<HyperAddrIncoming, IntoMakeServiceWithConnectInfo<Router, SocketAddr>>,
}

#[allow(dead_code)]
impl Server {
    pub async fn run(self) -> Result<(), Error> {
        self.server
            .await
            .map_err(|e| Error::Internal(format!("Server run failed: {}", e)))
    }

    pub async fn from_yaml_config(path: &str) -> Result<Self, crate::error::Error> {
        let config = ServerConfig::from_yaml(path)?;
        Self::from_config(&config).await
    }

    pub async fn from_config(config: &ServerConfig) -> Result<Self, Error> {
        let mut builder =
            ServerBuilder::new(config.connection.port, config.connection.host.clone());

        // TODO: remove rpc 1.0 dependency once DB work done
        let sui_sdk_client_v0 = sui_sdk_client_v0(&config.connection.rpc_url).await;
        let data_provider: Box<dyn DataProvider> = Box::new(sui_sdk_client_v0.clone());
        let data_loader = lru_cache_data_loader(&sui_sdk_client_v0).await;

        let name_service_config = config.name_service.clone();
        let pg_conn_pool = PgManager::new(config.connection.db_url.clone(), None)
            .map_err(|e| Error::Internal(format!("Failed to create pg connection pool: {}", e)))?;

        let prom_addr: SocketAddr = format!(
            "{}:{}",
            config.connection.prom_url, config.connection.prom_port
        )
        .parse()
        .map_err(|_| {
            Error::Internal(format!(
                "Failed to parse url {}, port {} into socket address",
                config.connection.prom_url, config.connection.prom_port
            ))
        })?;
        let registry_service = mysten_metrics::start_prometheus_server(prom_addr);
        println!("Starting Prometheus HTTP endpoint at {}", prom_addr);
        let registry = registry_service.default_registry();

        let metrics = RequestMetrics::new(&registry);

        builder = builder
            .max_query_depth(config.service.limits.max_query_depth)
            .max_query_nodes(config.service.limits.max_query_nodes)
            .context_data(data_provider)
            .context_data(data_loader)
            .context_data(pg_conn_pool)
            .context_data(name_service_config)
            .context_data(Arc::new(metrics))
            .context_data(config.clone());

        if config.internal_features.feature_gate {
            builder = builder.extension(FeatureGate);
        }
        if config.internal_features.logger {
            builder = builder.extension(Logger::default());
        }
        if config.internal_features.query_limits_checker {
            builder = builder.extension(QueryLimitsChecker::default());
        }
        if config.internal_features.query_timeout {
            builder = builder.extension(Timeout::default());
        }

        builder.build()
    }
}

pub(crate) struct ServerBuilder {
    port: u16,
    host: String,

    schema: SchemaBuilder<Query, EmptyMutation, EmptySubscription>,
}

impl ServerBuilder {
    pub fn new(port: u16, host: String) -> Self {
        Self {
            port,
            host,
            schema: async_graphql::Schema::build(Query, EmptyMutation, EmptySubscription),
        }
    }

    pub fn address(&self) -> String {
        format!("{}:{}", self.host, self.port)
    }

    pub fn max_query_depth(mut self, max_depth: u32) -> Self {
        self.schema = self.schema.limit_depth(max_depth as usize);
        self
    }

    pub fn max_query_nodes(mut self, max_nodes: u32) -> Self {
        self.schema = self.schema.limit_complexity(max_nodes as usize);
        self
    }

    pub fn context_data(mut self, context_data: impl Any + Send + Sync) -> Self {
        self.schema = self.schema.data(context_data);
        self
    }

    pub fn extension(mut self, extension: impl ExtensionFactory) -> Self {
        self.schema = self.schema.extension(extension);
        self
    }

    fn build_schema(self) -> Schema<Query, EmptyMutation, EmptySubscription> {
        self.schema.finish()
    }

    pub fn build(self) -> Result<Server, Error> {
        let address = self.address();
        let schema = self.build_schema();

        let app = axum::Router::new()
            .route("/", axum::routing::get(graphiql).post(graphql_handler))
            .layer(axum::extract::Extension(schema))
            .layer(middleware::from_fn(check_version_middleware))
            .layer(middleware::from_fn(set_version_middleware));
        Ok(Server {
            server: axum::Server::bind(
                &address
                    .parse()
                    .map_err(|_| Error::Internal(format!("Failed to parse address {}", address)))?,
            )
            .serve(app.into_make_service_with_connect_info::<SocketAddr>()),
        })
    }
}

async fn graphql_handler(
    ConnectInfo(addr): ConnectInfo<SocketAddr>,
    schema: axum::Extension<SuiGraphQLSchema>,
    headers: HeaderMap,
    req: GraphQLRequest,
) -> GraphQLResponse {
    let mut req = req.into_inner();
    if headers.contains_key(ShowUsage::name()) {
        req.data.insert(ShowUsage)
    }
    // Capture the IP address of the client
    // Note: if a load balancer is used it must be configured to forward the client IP address
    req.data.insert(addr);
    schema.execute(req).await.into()
}

async fn graphiql() -> impl axum::response::IntoResponse {
    axum::response::Html(
        async_graphql::http::GraphiQLSource::build()
            .endpoint("/")
            .finish(),
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{
        config::ServiceConfig,
        context_data::{
            data_provider::DataProvider, db_data_provider::PgManager,
            sui_sdk_data_provider::sui_sdk_client_v0,
        },
        extensions::query_limits_checker::QueryLimitsChecker,
        extensions::timeout::{Timeout, TimeoutConfig},
        metrics::RequestMetrics,
    };
    use async_graphql::{
        extensions::{Extension, ExtensionContext, NextExecute},
        Response,
    };
    use std::time::Duration;
    use std::{env, sync::Arc};

    #[ignore]
    #[tokio::test]
    async fn test_timeout() {
        struct TimedExecuteExt {
            pub min_req_delay: Duration,
        }

        impl ExtensionFactory for TimedExecuteExt {
            fn create(&self) -> Arc<dyn Extension> {
                Arc::new(TimedExecuteExt {
                    min_req_delay: self.min_req_delay,
                })
            }
        }

        #[async_trait::async_trait]
        impl Extension for TimedExecuteExt {
            async fn execute(
                &self,
                ctx: &ExtensionContext<'_>,
                operation_name: Option<&str>,
                next: NextExecute<'_>,
            ) -> Response {
                tokio::time::sleep(self.min_req_delay).await;
                next.run(ctx, operation_name).await
            }
        }

        async fn test_timeout(delay: Duration, timeout: Duration) -> Response {
            let sdk = sui_sdk_client_v0("https://fullnode.testnet.sui.io:443/").await;
            let data_provider: Box<dyn DataProvider> = Box::new(sdk);
            let db_url = env::var("PG_DB_URL").expect("PG_DB_URL must be set");
            let pg_conn_pool = PgManager::new(db_url, None)
                .map_err(|e| {
                    println!("Failed to create pg connection pool: {}", e);
                    e
                })
                .unwrap();
            let schema = ServerBuilder::new(8000, "127.0.0.1".to_string())
                .context_data(data_provider)
                .context_data(pg_conn_pool)
                .extension(TimedExecuteExt {
                    min_req_delay: delay,
                })
                .extension(Timeout {
                    config: TimeoutConfig {
                        request_timeout: timeout,
                    },
                })
                .build_schema();
            schema.execute("{ chainIdentifier }").await
        }

        let timeout = Duration::from_millis(1000);
        let delay = Duration::from_millis(100);

        // Should complete successfully
        let resp = test_timeout(delay, timeout).await;
        assert!(resp.is_ok());

        // Should timeout
        let errs: Vec<_> = test_timeout(timeout, timeout)
            .await
            .into_result()
            .unwrap_err()
            .into_iter()
            .map(|e| e.message)
            .collect();
        let exp = format!("Request timed out. Limit: {}s", timeout.as_secs_f32());
        assert_eq!(errs, vec![exp]);
    }

    #[ignore]
    #[tokio::test]
    async fn test_query_depth_limit() {
        async fn exec_query_depth_limit(depth: u32, query: &str) -> Response {
            let sdk = sui_sdk_client_v0("https://fullnode.testnet.sui.io:443/").await;
            let data_provider: Box<dyn DataProvider> = Box::new(sdk);
            let db_url = env::var("PG_DB_URL").expect("PG_DB_URL must be set");
            let pg_conn_pool = PgManager::new(db_url, None)
                .map_err(|e| {
                    println!("Failed to create pg connection pool: {}", e);
                    e
                })
                .unwrap();
            let schema = ServerBuilder::new(8000, "127.0.0.1".to_string())
                .context_data(data_provider)
                .context_data(pg_conn_pool)
                .max_query_depth(depth)
                .build_schema();
            schema.execute(query).await
        }

        // Should complete successfully
        let resp = exec_query_depth_limit(1, "{ chainIdentifier }").await;
        assert!(resp.is_ok());
        let resp = exec_query_depth_limit(
            5,
            "{ chainIdentifier protocolConfig { configs { value key }} }",
        )
        .await;
        assert!(resp.is_ok());

        // Should fail
        let errs: Vec<_> = exec_query_depth_limit(0, "{ chainIdentifier }")
            .await
            .into_result()
            .unwrap_err()
            .into_iter()
            .map(|e| e.message)
            .collect();

        assert_eq!(errs, vec!["Query is nested too deep.".to_string()]);
        let errs: Vec<_> = exec_query_depth_limit(
            2,
            "{ chainIdentifier protocolConfig { configs { value key }} }",
        )
        .await
        .into_result()
        .unwrap_err()
        .into_iter()
        .map(|e| e.message)
        .collect();
        assert_eq!(errs, vec!["Query is nested too deep.".to_string()]);
    }

    #[ignore]
    #[tokio::test]
    async fn test_query_node_limit() {
        async fn exec_query_node_limit(nodes: u32, query: &str) -> Response {
            let sdk = sui_sdk_client_v0("https://fullnode.testnet.sui.io:443/").await;
            let data_provider: Box<dyn DataProvider> = Box::new(sdk);
            let db_url = env::var("PG_DB_URL").expect("PG_DB_URL must be set");
            let pg_conn_pool = PgManager::new(db_url, None)
                .map_err(|e| {
                    println!("Failed to create pg connection pool: {}", e);
                    e
                })
                .unwrap();
            let schema = ServerBuilder::new(8000, "127.0.0.1".to_string())
                .context_data(data_provider)
                .context_data(pg_conn_pool)
                .max_query_nodes(nodes)
                .build_schema();
            schema.execute(query).await
        }

        // Should complete successfully
        let resp = exec_query_node_limit(1, "{ chainIdentifier }").await;
        assert!(resp.is_ok());
        let resp = exec_query_node_limit(
            5,
            "{ chainIdentifier protocolConfig { configs { value key }} }",
        )
        .await;
        assert!(resp.is_ok());

        // Should fail
        let err: Vec<_> = exec_query_node_limit(0, "{ chainIdentifier }")
            .await
            .into_result()
            .unwrap_err()
            .into_iter()
            .map(|e| e.message)
            .collect();
        assert_eq!(err, vec!["Query is too complex.".to_string()]);

        let err: Vec<_> = exec_query_node_limit(
            4,
            "{ chainIdentifier protocolConfig { configs { value key }} }",
        )
        .await
        .into_result()
        .unwrap_err()
        .into_iter()
        .map(|e| e.message)
        .collect();
        assert_eq!(err, vec!["Query is too complex.".to_string()]);
    }

    #[ignore]
    #[tokio::test]
    async fn test_query_complexity_metrics() {
        let binding_address: SocketAddr = "0.0.0.0:9184".parse().unwrap();
        let registry = mysten_metrics::start_prometheus_server(binding_address).default_registry();
        let metrics = RequestMetrics::new(&registry);
        let metrics = Arc::new(metrics);
        let metrics2 = metrics.clone();

        let service_config = ServiceConfig::default();
        let sdk = sui_sdk_client_v0("https://fullnode.testnet.sui.io:443/").await;
        let data_provider: Box<dyn DataProvider> = Box::new(sdk);

        let db_url = env::var("PG_DB_URL").expect("PG_DB_URL must be set");
        let pg_conn_pool = PgManager::new(db_url, None)
            .map_err(|e| {
                println!("Failed to create pg connection pool: {}", e);
                e
            })
            .unwrap();
        let schema = ServerBuilder::new(8000, "127.0.0.1".to_string())
            .max_query_depth(service_config.limits.max_query_depth)
            .max_query_nodes(service_config.limits.max_query_nodes)
            .context_data(service_config)
            .context_data(data_provider)
            .context_data(pg_conn_pool)
            .context_data(metrics)
            .extension(QueryLimitsChecker::default())
            .build_schema();
        let _ = schema.execute("{ chainIdentifier }").await;

        assert_eq!(metrics2.num_nodes.get_sample_count(), 1);
        assert_eq!(metrics2.query_depth.get_sample_count(), 1);
        assert_eq!(metrics2.num_nodes.get_sample_sum(), 1.);
        assert_eq!(metrics2.query_depth.get_sample_sum(), 1.);

        let _ = schema
            .execute("{ chainIdentifier protocolConfig { configs { value key }} }")
            .await;
        assert_eq!(metrics2.num_nodes.get_sample_count(), 2);
        assert_eq!(metrics2.query_depth.get_sample_count(), 2);
        assert_eq!(metrics2.num_nodes.get_sample_sum(), 2. + 4.);
        assert_eq!(metrics2.query_depth.get_sample_sum(), 1. + 3.);
    }
}
