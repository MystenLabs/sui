// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

mod oauth;

pub use oauth::get_oauth_token;

pub fn get_api_server() -> String {
    // if env var is set, use that
    if let Ok(api_server) = std::env::var("API_SERVER") {
        return api_server.to_string();
    }

    "https://meta-svc.api.mystenlabs.com".to_string()
}
