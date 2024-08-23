// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use axum::extract::Path;
use tokio_util::sync::CancellationToken;
use tracing::info;

use crate::config::{ServerConfig, Version};
use crate::error::Error;
use crate::server::builder::ServerBuilder;

async fn graphiql(
    ide_title: axum::Extension<Option<String>>,
    path: Option<Path<String>>,
) -> impl axum::response::IntoResponse {
    let endpoint = if let Some(Path(path)) = path {
        format!("/graphql/{}", path)
    } else {
        "/graphql".to_string()
    };
    let gq = async_graphql::http::GraphiQLSource::build().endpoint(&endpoint);
    if let axum::Extension(Some(title)) = ide_title {
        axum::response::Html(gq.title(&title).finish())
    } else {
        axum::response::Html(gq.finish())
    }
}

pub async fn start_graphiql_server(
    server_config: &ServerConfig,
    version: &Version,
    cancellation_token: CancellationToken,
) -> Result<(), Error> {
    info!("Starting server with config: {:#?}", server_config);
    info!("Server version: {}", version);
    start_graphiql_server_impl(
        ServerBuilder::from_config(server_config, version, cancellation_token).await?,
        server_config.ide.ide_title.clone(),
    )
    .await
}

async fn start_graphiql_server_impl(
    server_builder: ServerBuilder,
    ide_title: String,
) -> Result<(), Error> {
    let address = server_builder.address();

    // Add GraphiQL IDE handler on GET request to `/`` endpoint
    let server = server_builder
        .route("/", axum::routing::get(graphiql))
        .route("/:version", axum::routing::get(graphiql))
        .route("/graphql", axum::routing::get(graphiql))
        .route("/graphql/:version", axum::routing::get(graphiql))
        .layer(axum::extract::Extension(Some(ide_title)))
        .build()?;

    info!("Launch GraphiQL IDE at: http://{}", address);

    server.run().await
}
