// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

mod util;

use std::net::SocketAddr;

use anyhow::Result;
use axum::response::IntoResponse;
use axum::{extract::Query, routing::get, Router};
use chrono;
use dirs;
use reqwest::header::{HeaderMap, HeaderValue};
use serde::{Deserialize, Serialize};
use std::fs::{self, File};
use std::io::{self, Read, Write};
use std::path::Path;
use tokio::sync::mpsc;
use tokio::task;
use tracing::{debug, info};

// Okta client created for Mysten Labs
// Contact #techops-support for replacement
const CLIENT_ID: &str = "0oacw4bwt1BOV410t697";

#[derive(Deserialize)]
pub struct TokenResponse {
    pub access_token: String,
    pub expires_in: u64,
}
#[derive(Serialize, Deserialize)]
pub struct TokenData {
    pub access_token: String,
    pub expires_at: i64,
}

impl TokenData {
    fn from_file(file_path: &str) -> io::Result<Self> {
        let mut file = File::open(file_path)?;
        let mut contents = String::new();
        file.read_to_string(&mut contents)?;
        serde_json::from_str(&contents).map_err(|e| io::Error::new(io::ErrorKind::InvalidData, e))
    }

    // Function to write token data to file
    fn to_file(&self, file_path: &str) -> io::Result<()> {
        let serialized = serde_json::to_string(self)?;
        let mut file = File::create(file_path)?;
        file.write_all(serialized.as_bytes())?;
        Ok(())
    }

    // Function to check if the token is expired
    fn is_expired(&self) -> bool {
        // compare with current time
        chrono::Utc::now().timestamp() > self.expires_at
    }
}

fn get_token_file_path() -> String {
    let home = dirs::home_dir().unwrap();
    let token_file_path = format!("{}/.suiop/okta_token.json", home.display());
    fs::create_dir_all(Path::new(&token_file_path).parent().unwrap()).unwrap();
    token_file_path
}

pub async fn get_oauth_token() -> Result<TokenData> {
    // check if token is saved and not expired
    let token_file_path = get_token_file_path();
    let saved_token = TokenData::from_file(&token_file_path);
    if let Ok(token) = saved_token {
        if !token.is_expired() {
            debug!("Using saved token.");
            return Ok(token);
        }
    }

    let pkce_code = util::generate_pkce_code();
    let state = util::generate_state();
    let (sender, mut receiver) = mpsc::channel::<TokenData>(1);
    let server_task = spawn_local_server(sender.clone(), pkce_code.code_verifier.clone());
    util::request_authorization_code(CLIENT_ID, &state, &pkce_code.code_challenge)?;

    if let Some(token) = receiver.recv().await {
        debug!("[rx] Received access code: {}", token.access_token);
        debug!("[rx] Expires At: {}", token.expires_at);
        info!("Finished authorization flow. Cleaning up...");
        let _ = token.to_file(&token_file_path);
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
    sender: mpsc::Sender<TokenData>,
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

    let token_data = TokenData {
        access_token: response.access_token.clone(),
        expires_at: chrono::Utc::now().timestamp() + response.expires_in as i64,
    };
    sender.send(token_data).await.unwrap();

    "Access code received. You can close this window now."
}

fn spawn_local_server(
    sender: mpsc::Sender<TokenData>,
    code_verifier: String,
) -> task::JoinHandle<()> {
    task::spawn(async move {
        let app = Router::new().route(
            "/callback",
            get(move |params| authorize_callback_handler(sender, params, code_verifier)),
        );
        let addr = SocketAddr::from(([127, 0, 0, 1], 17846));

        let listener = tokio::net::TcpListener::bind(addr).await.unwrap();
        axum::serve(listener, app)
            .await
            .expect("couldn't start local auth server on port 17846");
    })
}
