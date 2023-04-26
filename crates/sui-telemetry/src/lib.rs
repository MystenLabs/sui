// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use std::sync::Arc;
use std::time::{SystemTime, UNIX_EPOCH};
use sui_core::authority::AuthorityState;
use tracing::trace;

use fastcrypto::encoding::Encoding;
use fastcrypto::encoding::Hex;

pub(crate) const GA_API_SECRET: &str = "zeq-aYEzS0aGdRJ8kNZTEg";
pub(crate) const GA_EVENT_NAME: &str = "node_telemetry_event";
pub(crate) const GA_MEASUREMENT_ID: &str = "G-96DM59YK2F";
pub(crate) const GA_URL: &str = "https://www.google-analytics.com/mp/collect";
// need this hardcoded client ID as only existing client is valid.
// see below for details:
// https://developers.google.com/analytics/devguides/collection/protocol/ga4/verify-implementation?client_type=gtag
pub(crate) const HARDCODED_CLIENT_ID: &str = "1871165366.1648333069";
pub(crate) const IPLOOKUP_URL: &str = "https://api.ipify.org?format=json";
pub(crate) const UNKNOWN_STRING: &str = "UNKNOWN";

#[derive(Debug, Serialize, Deserialize)]
struct TelemetryEvent {
    name: String,
    params: BTreeMap<String, String>,
}

// The payload needs to meet this requirement in
// https://developers.google.com/analytics/devguides/collection/protocol/ga4/reference?client_type=gtag#payload_post_body
#[derive(Debug, Serialize, Deserialize)]
struct TelemetryPayload {
    client_id: String,
    events: Vec<TelemetryEvent>,
}

#[derive(Debug, Serialize, Deserialize)]
struct IpResponse {
    ip: String,
}

pub async fn send_telemetry_event(state: Arc<AuthorityState>, is_validator: bool) {
    let git_rev = env!("CARGO_PKG_VERSION").to_string();
    let ip_address = get_ip().await;
    let chain_identifier = match state.get_chain_identifier() {
        // Unwrap safe: Checkpoint Digest is 32 bytes long
        Some(chain_identifier) => Hex::encode(chain_identifier.into_inner().get(0..4).unwrap()),
        None => "Unknown".to_string(),
    };
    let since_the_epoch = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .expect("Now should be later than epoch!");
    let telemetry_event = TelemetryEvent {
        name: GA_EVENT_NAME.into(),
        params: BTreeMap::from([
            ("chain_identifier".into(), chain_identifier),
            ("node_address".into(), ip_address),
            (
                "node_type".into(),
                if is_validator {
                    "validator".into()
                } else {
                    "full_node".into()
                },
            ),
            ("git_rev".into(), git_rev),
            (
                "seconds_since_epoch".into(),
                since_the_epoch.as_secs().to_string(),
            ),
        ]),
    };

    let telemetry_payload = TelemetryPayload {
        client_id: HARDCODED_CLIENT_ID.into(),
        events: vec![telemetry_event],
    };

    send_telemetry_event_impl(telemetry_payload).await
}

async fn get_ip() -> String {
    let resp = reqwest::get(IPLOOKUP_URL).await;
    match resp {
        Ok(json) => match json.json::<IpResponse>().await {
            Ok(ip_json) => ip_json.ip,
            Err(_) => UNKNOWN_STRING.into(),
        },
        Err(_) => UNKNOWN_STRING.into(),
    }
}

async fn send_telemetry_event_impl(telemetry_payload: TelemetryPayload) {
    let client = reqwest::Client::new();
    let response_result = client
        .post(format!(
            "{}?&measurement_id={}&api_secret={}",
            GA_URL, GA_MEASUREMENT_ID, GA_API_SECRET
        ))
        .json::<TelemetryPayload>(&telemetry_payload)
        .send()
        .await;

    match response_result {
        Ok(response) => {
            let status = response.status().as_u16();
            if (200..299).contains(&status) {
                trace!("SUCCESS: Sent telemetry event: {:?}", &telemetry_payload,);
            } else {
                trace!(
                    "FAIL: Sending telemetry event failed with status: {} and response {:?}.",
                    response.status(),
                    response
                );
            }
        }
        Err(error) => {
            trace!(
                "FAIL: Sending telemetry event failed with error: {:?}",
                error
            );
        }
    }
}
