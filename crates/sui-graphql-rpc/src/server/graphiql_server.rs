// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use tracing::info;

use crate::config::ServerConfig;
use crate::error::Error;
use crate::server::builder::ServerBuilder;

async fn graphiql(ide_title: axum::Extension<Option<String>>) -> impl axum::response::IntoResponse {
    let gq = async_graphql::http::GraphiQLSource::build().endpoint("/");
    if let axum::Extension(Some(title)) = ide_title {
        axum::response::Html(gq.title(&title).finish())
    } else {
        axum::response::Html(gq.finish())
    }
}

pub async fn start_graphiql_server(server_config: &ServerConfig) -> Result<(), Error> {
    info!("Starting server with config: {:?}", server_config);
    start_graphiql_server_impl(
        ServerBuilder::from_config(server_config).await?,
        server_config.ide.ide_title.clone(),
    )
    .await
}

pub async fn start_graphiql_server_from_cfg_path(server_config_path: &str) -> Result<(), Error> {
    let (server_builder, config) = ServerBuilder::from_yaml_config(server_config_path).await?;
    start_graphiql_server_impl(server_builder, config.ide.ide_title).await
}

async fn start_graphiql_server_impl(
    server_builder: ServerBuilder,
    ide_title: String,
) -> Result<(), Error> {
    let address = server_builder.address();

    // Add GraphiQL IDE handler on GET request to `/`` endpoint
    let server = server_builder
        .route("/", axum::routing::get(graphiql))
        .route("/graphql", axum::routing::get(graphiql))
        .layer(axum::extract::Extension(Some(ide_title)))
        .build()?;

    info!("Launch GraphiQL IDE at: http://{}", address);

    server.run().await
}
