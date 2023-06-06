// Copyright (c) 2021, Facebook, Inc. and its affiliates
// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0
use crate::error::{SuiError, SuiResult};
use fastcrypto_zkp::bn254::zk_login::OAuthProvider;
use im::hashmap::HashMap as ImHashMap;
use once_cell::sync::Lazy;
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use std::{collections::HashMap, hash::Hash, sync::Arc};

/// A whitelist of client_ids (i.e. the value of "aud" in cliams) for each provider
pub static DEFAULT_WHITELIST: Lazy<HashMap<&str, Vec<&str>>> = Lazy::new(|| {
    let mut map = HashMap::new();
    map.insert(
        OAuthProvider::Google.get_config().0,
        vec!["946731352276-pk5glcg8cqo38ndb39h7j093fpsphusu.apps.googleusercontent.com"],
    );
    map.insert(
        OAuthProvider::Twitch.get_config().0,
        vec!["d31icql6l8xzpa7ef31ztxyss46ock"],
    );
    map
});

/// Parameters for generating an address.
#[derive(Debug, Serialize, Deserialize)]
pub struct AddressParams {
    iss: String,
    key_claim_name: String,
}

impl AddressParams {
    pub fn new(iss: String, key_claim_name: String) -> Self {
        Self {
            iss,
            key_claim_name,
        }
    }
}

/// Struct that contains all the OAuth provider information. A list of them can
/// be retrieved from the JWK endpoint (e.g. <https://www.googleapis.com/oauth2/v3/certs>)
/// and published on the bulletin along with a trusted party's signature.
#[derive(Debug, Clone, PartialEq, Eq, JsonSchema, Hash, Serialize, Deserialize)]
pub struct OAuthProviderContent {
    kty: String,
    kid: String,
    pub e: String,
    pub n: String,
    alg: String,
}

#[derive(Debug, Clone, PartialEq, Eq, JsonSchema, Hash, Serialize, Deserialize)]
pub struct OAuthProviderContentReader {
    e: String,
    n: String,
    #[serde(rename = "use")]
    my_use: String,
    kid: String,
    kty: String,
    alg: String,
}

impl OAuthProviderContent {
    pub fn kid(&self) -> &str {
        &self.kid
    }

    pub fn from_reader(reader: OAuthProviderContentReader) -> Self {
        Self {
            kty: reader.kty,
            kid: reader.kid,
            e: trim(reader.e),
            n: trim(reader.n),
            alg: reader.alg,
        }
    }
}

/// Trim trailing '=' so that it is considered a valid base64 url encoding string by base64ct library.
fn trim(str: String) -> String {
    str.trim_end_matches(|c: char| c == '=').to_owned()
}

/// Parse the JWK bytes received from the oauth provider keys endpoint into a map from kid to
/// OAuthProviderContent.
pub fn parse_jwks(json_bytes: &[u8]) -> SuiResult<ImHashMap<String, Arc<OAuthProviderContent>>> {
    let json_str = String::from_utf8_lossy(json_bytes);
    let parsed_list: Result<serde_json::Value, serde_json::Error> = serde_json::from_str(&json_str);
    if let Ok(parsed_list) = parsed_list {
        if let Some(keys) = parsed_list["keys"].as_array() {
            let mut ret = ImHashMap::new();
            for k in keys {
                let parsed: OAuthProviderContentReader =
                    serde_json::from_value(k.clone()).map_err(|_| SuiError::JWKRetrievalError)?;

                if parsed.alg == "RS256" && parsed.my_use == "sig" && parsed.kty == "RSA" {
                    ret.insert(
                        parsed.kid.clone(),
                        Arc::new(OAuthProviderContent::from_reader(parsed)),
                    );
                }
            }
            return Ok(ret);
        }
    }
    Err(SuiError::JWKRetrievalError)
}
