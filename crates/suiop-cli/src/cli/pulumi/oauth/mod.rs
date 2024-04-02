// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

mod util;

use std::net::SocketAddr;

use anyhow::Result;
use axum::http::{HeaderMap, HeaderValue};
use axum::response::IntoResponse;
use axum::{extract::Query, routing::get, Router};
use reqwest;
use serde::Deserialize;
use tokio::sync::mpsc;
use tokio::task;
use tracing::{debug, info};

// use tokio::time::{sleep, Duration};

const CLIENT_ID: &str = "0oacw4bwt1BOV410t697";

#[derive(Deserialize)]
pub struct TokenResponse {
    pub access_token: String,
    pub token_type: String,
    pub expires_in: u64,
    pub scope: String,
    pub id_token: String,
}
pub async fn get_oauth_token() -> Result<TokenResponse> {
    let pkce_code = util::generate_pkce_code();
    let state = util::generate_state();
    let (sender, mut receiver) = mpsc::channel::<TokenResponse>(1);
    let server_task = spawn_local_server(sender.clone(), pkce_code.code_verifier.clone());
    util::request_authorization_code(CLIENT_ID, &state, &pkce_code.code_challenge)?;

    if let Some(token) = receiver.recv().await {
        debug!("[rx] Received access code: {}", token.access_token);
        info!("Finished authorization flow. Cleaning up...");
        server_task.abort();
        Ok(token)
    } else {
        server_task.abort();
        Err(anyhow::anyhow!("Failed to receive oauth token."))
    }
}

#[derive(Deserialize)]
struct CallbackParam {
    code: String,
}
async fn authorize_callback_handler(
    sender: mpsc::Sender<TokenResponse>,
    params: Query<CallbackParam>,
    code_verifier: String,
) -> impl IntoResponse {
    // exchange authorization code for access token
    let url = format!(
        "https://mystenlabs.okta.com/oauth2/v1/token?\
            &grant_type=authorization_code\
            &client_id={}\
            &code={}\
            &code_verifier={}\
            &redirect_uri=http://127.0.0.1:17846/callback",
        CLIENT_ID,
        params.code.clone(),
        code_verifier,
    );

    let mut headers = HeaderMap::new();
    headers.insert("accept", HeaderValue::from_str("application/json").unwrap());
    headers.insert("cache-control", HeaderValue::from_str("no-cache").unwrap());
    headers.insert(
        "content-type",
        HeaderValue::from_str("application/x-www-form-urlencoded").unwrap(),
    );

    let response = reqwest::Client::new()
        .post(&url)
        .headers(headers)
        .send()
        .await
        .unwrap()
        .json::<TokenResponse>()
        .await
        .unwrap();

    sender.send(response).await.unwrap();

    "Access code received. You can close this window now."
}

fn spawn_local_server(
    sender: mpsc::Sender<TokenResponse>,
    code_verifier: String,
) -> task::JoinHandle<()> {
    task::spawn(async move {
        let app = Router::new().route(
            "/callback",
            get(move |params| authorize_callback_handler(sender, params, code_verifier)),
        );
        let addr = SocketAddr::from(([127, 0, 0, 1], 17846));

        axum::Server::bind(&addr)
            .serve(app.into_make_service())
            .await
            .unwrap();
    })
}
