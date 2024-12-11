// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use mysten_metrics::RegistryService;
use prometheus::Encoder;
use std::time::{Duration, SystemTime, UNIX_EPOCH};
use tracing::{debug, error, info};

const METRICS_PUSH_TIMEOUT: Duration = Duration::from_secs(45);

pub struct MetricsPushClient {
    certificate: std::sync::Arc<sui_tls::SelfSignedCertificate>,
    client: reqwest::Client,
}

impl MetricsPushClient {
    pub fn new(metrics_key: sui_types::crypto::NetworkKeyPair) -> Self {
        use fastcrypto::traits::KeyPair;
        let certificate = std::sync::Arc::new(sui_tls::SelfSignedCertificate::new(
            metrics_key.private(),
            sui_tls::SUI_VALIDATOR_SERVER_NAME,
        ));
        let identity = certificate.reqwest_identity();
        let client = reqwest::Client::builder()
            .identity(identity)
            .build()
            .unwrap();

        Self {
            certificate,
            client,
        }
    }

    pub fn certificate(&self) -> &sui_tls::SelfSignedCertificate {
        &self.certificate
    }

    pub fn client(&self) -> &reqwest::Client {
        &self.client
    }
}

pub async fn push_metrics(
    client: &MetricsPushClient,
    url: &reqwest::Url,
    registry: &RegistryService,
) -> Result<(), anyhow::Error> {
    info!(push_url =% url, "pushing metrics to remote");

    // now represents a collection timestamp for all of the metrics we send to the proxy
    let now = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_millis() as i64;

    let mut metric_families = registry.gather_all();
    for mf in metric_families.iter_mut() {
        for m in mf.mut_metric() {
            m.set_timestamp_ms(now);
        }
    }

    let mut buf: Vec<u8> = vec![];
    let encoder = prometheus::ProtobufEncoder::new();
    encoder.encode(&metric_families, &mut buf)?;

    let mut s = snap::raw::Encoder::new();
    let compressed = s.compress_vec(&buf).map_err(|err| {
        error!("unable to snappy encode; {err}");
        err
    })?;

    let response = client
        .client()
        .post(url.to_owned())
        .header(reqwest::header::CONTENT_ENCODING, "snappy")
        .header(reqwest::header::CONTENT_TYPE, prometheus::PROTOBUF_FORMAT)
        .body(compressed)
        .timeout(METRICS_PUSH_TIMEOUT)
        .send()
        .await?;

    if !response.status().is_success() {
        let status = response.status();
        let body = match response.text().await {
            Ok(body) => body,
            Err(error) => format!("couldn't decode response body; {error}"),
        };
        return Err(anyhow::anyhow!(
            "metrics push failed: [{}]:{}",
            status,
            body
        ));
    }

    debug!("successfully pushed metrics to {url}");

    Ok(())
}
