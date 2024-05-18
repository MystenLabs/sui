// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use open;

use anyhow::Result;
use base64::{engine::general_purpose::URL_SAFE_NO_PAD, Engine as _};
use rand::distributions::Alphanumeric;
use rand::{thread_rng, Rng};
use sha2::{Digest, Sha256};
use tracing::info;

pub struct PKCECode {
    pub code_verifier: String,
    pub code_challenge: String,
}

pub fn generate_pkce_code() -> PKCECode {
    let code_len: usize = thread_rng().gen_range(43..128);
    let code_verifier: String = thread_rng()
        .sample_iter(&Alphanumeric)
        .take(code_len)
        .map(char::from)
        .collect();
    let sha256_digest = Sha256::digest(code_verifier.as_bytes());

    let code_challenge = URL_SAFE_NO_PAD.encode(sha256_digest);

    PKCECode {
        code_verifier,
        code_challenge,
    }
}

pub fn generate_state() -> String {
    thread_rng()
        .sample_iter(&Alphanumeric)
        .take(43)
        .map(char::from)
        .collect()
}

pub fn request_authorization_code(
    client_id: &str,
    state: &str,
    code_challenge: &str,
) -> Result<()> {
    let url = format!(
        "https://mystenlabs.okta.com/oauth2/v1/authorize?\
        &client_id={}\
        &response_type=code\
        &scope=openid\
        &redirect_uri=http://127.0.0.1:17846/callback\
        &state={}\
        &code_challenge_method=S256\
        &code_challenge={}",
        client_id, state, &code_challenge
    );

    info!(
        "Opening the following URL in your browser to authenticate: {}",
        url
    );
    open::that(url)?;
    Ok(())
}
