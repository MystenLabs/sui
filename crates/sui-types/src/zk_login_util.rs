// Copyright (c) 2021, Facebook, Inc. and its affiliates
// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0
use crate::error::{SuiError, SuiResult};
use fastcrypto_zkp::bn254::zk_login::OAuthProvider;
use once_cell::sync::Lazy;
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use std::{collections::HashMap, hash::Hash};

// Used in tests or anywhere that fetching up to date JWKs is not possible.
pub const DEFAULT_JWK_BYTES: &[u8] = r#"{
    "keys": [
        {
          "kty": "RSA",
          "e": "AQAB",
          "alg": "RS256",
          "kid": "2d9a5ef5b12623c91671a7093cb323333cd07d09",
          "use": "sig",
          "n": "0NDRXWtH6_HnmuSuTAisgYVZ3Z67PQjHbRFz4XNYuD95BKx0wQr0GWOi_UCGLfI0col3i6J3_AF-b1YrTFTMEr_bL8CYDdK2CYLcGUzc5bLRDAySsqnKdlhWkneqfFdr3J66mHu11KUaIIRWiLsCkR9QFF-8o2PtZzv3F-3Uh7L4q7i_Evs1s7SJlO0OAnI4ew4rP2HbRaO0Q2zK0DL_d1eoAC72apQuEzz-2aXfQ-QYSTlVK74McBhP1MRtgD6zGF2lwg4uhgb55fDDQQh0VHWQSxwbvAL0Oox69zzpkFgpjJAJUqaxegzETU1jf3iKs1vyFIB0C4N-Jr__zwLQZw=="
        },
        {
          "alg": "RS256",
          "use": "sig",
          "n": "1qrQCTst3RF04aMC9Ye_kGbsE0sftL4FOtB_WrzBDOFdrfVwLfflQuPX5kJ-0iYv9r2mjD5YIDy8b-iJKwevb69ISeoOrmL3tj6MStJesbbRRLVyFIm_6L7alHhZVyqHQtMKX7IaNndrfebnLReGntuNk76XCFxBBnRaIzAWnzr3WN4UPBt84A0KF74pei17dlqHZJ2HB2CsYbE9Ort8m7Vf6hwxYzFtCvMCnZil0fCtk2OQ73l6egcvYO65DkAJibFsC9xAgZaF-9GYRlSjMPd0SMQ8yU9i3W7beT00Xw6C0FYA9JAYaGaOvbT87l_6ZkAksOMuvIPD_jNVfTCPLQ==",
          "e": "AQAB",
          "kty": "RSA",
          "kid": "6083dd5981673f661fde9dae646b6f0380a0145c"
        }
      ]
  }"#.as_bytes();

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
pub fn parse_jwks(json_bytes: &[u8]) -> SuiResult<Vec<(String, OAuthProviderContent)>> {
    let json_str = String::from_utf8_lossy(json_bytes);
    let parsed_list: Result<serde_json::Value, serde_json::Error> = serde_json::from_str(&json_str);
    if let Ok(parsed_list) = parsed_list {
        if let Some(keys) = parsed_list["keys"].as_array() {
            let mut ret = Vec::new();
            for k in keys {
                let parsed: OAuthProviderContentReader =
                    serde_json::from_value(k.clone()).map_err(|_| SuiError::JWKRetrievalError)?;

                if parsed.alg == "RS256" && parsed.my_use == "sig" && parsed.kty == "RSA" {
                    ret.push((
                        parsed.kid.clone(),
                        OAuthProviderContent::from_reader(parsed),
                    ));
                }
            }
            return Ok(ret);
        }
    }
    Err(SuiError::JWKRetrievalError)
}
