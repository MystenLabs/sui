// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use crate::admin::ReqwestClient;
use crate::prom_to_mimir::Mimir;
use crate::remote_write::WriteRequest;
use anyhow::Result;
use axum::body::Bytes;
use axum::http::StatusCode;
use bytes::buf::Reader;
use fastcrypto::ed25519::Ed25519PublicKey;
use multiaddr::Multiaddr;
use once_cell::sync::Lazy;
use prometheus::proto;
use prometheus::{register_counter, register_counter_vec, register_histogram_vec};
use prometheus::{Counter, CounterVec, HistogramVec};
use prost::Message;
use protobuf::CodedInputStream;
use std::io::Read;
use tracing::{debug, error};

static CONSUMER_OPS_SUBMITTED: Lazy<Counter> = Lazy::new(|| {
    register_counter!(
        "consumer_operations_submitted",
        "Operations counter for the number of metric family types we submit, excluding histograms, and not the discrete timeseries counts.",
    )
    .unwrap()
});

static CONSUMER_OPS: Lazy<CounterVec> = Lazy::new(|| {
    register_counter_vec!(
        "consumer_operations",
        "Operations counters and status from operations performed in the consumer.",
        &["operation", "status"]
    )
    .unwrap()
});
static CONSUMER_ENCODE_COMPRESS_DURATION: Lazy<HistogramVec> = Lazy::new(|| {
    register_histogram_vec!(
        "protobuf_compression_seconds",
        "The time it takes to compress a remote_write payload in seconds.",
        &["operation"],
        vec![
            1e-08, 2e-08, 4e-08, 8e-08, 1.6e-07, 3.2e-07, 6.4e-07, 1.28e-06, 2.56e-06, 5.12e-06,
            1.024e-05, 2.048e-05, 4.096e-05, 8.192e-05
        ],
    )
    .unwrap()
});
static CONSUMER_OPERATION_DURATION: Lazy<HistogramVec> = Lazy::new(|| {
    register_histogram_vec!(
        "consumer_operations_duration_seconds",
        "The time it takes to perform various consumer operations in seconds.",
        &["operation"],
        vec![
            0.0008, 0.0016, 0.0032, 0.0064, 0.0128, 0.0256, 0.0512, 0.1024, 0.2048, 0.4096, 0.8192,
            1.0, 1.25, 1.5, 1.75, 2.0, 4.0, 8.0, 10.0, 12.5, 15.0
        ],
    )
    .unwrap()
});

/// NodeMetric holds metadata and a metric payload from the calling node
#[derive(Debug)]
pub struct NodeMetric {
    pub peer_addr: Multiaddr, // the sockaddr source address from the incoming request
    pub public_key: Ed25519PublicKey, // the public key from the sui blockchain
    pub data: Vec<proto::MetricFamily>, // decoded protobuf of prometheus data
}

/// The ProtobufDecoder will decode message delimited protobuf messages from prom_model.proto types
/// They are delimited by size, eg a format is such:
/// []byte{size, data, size, data, size, data}, etc etc
pub struct ProtobufDecoder {
    buf: Reader<Bytes>,
}

impl ProtobufDecoder {
    pub fn new(buf: Reader<Bytes>) -> Self {
        Self { buf }
    }
    /// parse a delimited buffer of protobufs. this is used to consume data sent from a sui-node
    pub fn parse<T: protobuf::Message>(&mut self) -> Result<Vec<T>> {
        let timer = CONSUMER_OPERATION_DURATION
            .with_label_values(&["decode_len_delim_protobuf"])
            .start_timer();
        let mut result: Vec<T> = vec![];
        while !self.buf.get_ref().is_empty() {
            let len = {
                let mut is = CodedInputStream::from_buffered_reader(&mut self.buf);
                is.read_raw_varint32()
            }?;
            let mut buf = vec![0; len as usize];
            self.buf.read_exact(&mut buf)?;
            result.push(T::parse_from_bytes(&buf)?);
        }
        timer.observe_duration();
        Ok(result)
    }
}

// populate labels in place for our given metric family data
pub fn populate_labels(
    name: String,               // host field for grafana agent (from chain data)
    network: String,            // network name from ansible (via config)
    inventory_hostname: String, // inventory_name from ansible (via config)
    data: Vec<proto::MetricFamily>,
) -> Vec<proto::MetricFamily> {
    let timer = CONSUMER_OPERATION_DURATION
        .with_label_values(&["populate_labels"])
        .start_timer();
    // proto::LabelPair doesn't have pub fields so we can't use
    // struct literals to construct
    let mut network_label = proto::LabelPair::default();
    network_label.set_name("network".into());
    network_label.set_value(network);

    let mut host_label = proto::LabelPair::default();
    host_label.set_name("host".into());
    host_label.set_value(name);

    let mut relay_host_label = proto::LabelPair::default();
    relay_host_label.set_name("relay_host".into());
    relay_host_label.set_value(inventory_hostname);

    let labels = vec![network_label, host_label, relay_host_label];

    let mut data = data;
    // add our extra labels to our incoming metric data
    for mf in data.iter_mut() {
        for m in mf.mut_metric() {
            m.mut_label().extend(labels.clone());
        }
    }
    timer.observe_duration();
    data
}

fn encode_compress(request: &WriteRequest) -> Result<Vec<u8>, (StatusCode, &'static str)> {
    let observe = || {
        let timer = CONSUMER_ENCODE_COMPRESS_DURATION
        .with_label_values(&["encode_compress"])
        .start_timer();
    ||{
        timer.observe_duration();
    }
    }();
    let mut buf = Vec::new();
    buf.reserve(request.encoded_len());
    if request.encode(&mut buf).is_err() {
        observe();
        CONSUMER_OPS
            .with_label_values(&["encode_compress", "failed"])
            .inc();
        error!("unable to encode prompb to mimirpb");
        return Err((
            StatusCode::INTERNAL_SERVER_ERROR,
            "unable to encode prompb to remote_write pb",
        ));
    };

    let mut s = snap::raw::Encoder::new();
    let compressed = match s.compress_vec(&buf) {
        Ok(compressed) => compressed,
        Err(error) => {
            observe();
            CONSUMER_OPS
                .with_label_values(&["encode_compress", "failed"])
                .inc();
            error!("unable to compress to snappy block format; {error}");
            return Err((
                StatusCode::INTERNAL_SERVER_ERROR,
                "unable to compress to snappy block format",
            ));
        }
    };
    observe();
    CONSUMER_OPS
        .with_label_values(&["encode_compress", "success"])
        .inc();
    Ok(compressed)
}

async fn check_response(
    request: WriteRequest,
    response: reqwest::Response,
) -> Result<(), (StatusCode, &'static str)> {
    match response.status() {
        reqwest::StatusCode::OK => {
            CONSUMER_OPS
                .with_label_values(&["check_response", "OK"])
                .inc();
            debug!("({}) SUCCESS: {:?}", reqwest::StatusCode::OK, request);
            Ok(())
        }
        reqwest::StatusCode::BAD_REQUEST => {
            error!("TRIED: {:?}", request);
            let body = response
                .text()
                .await
                .unwrap_or_else(|_| "response body cannot be decoded".into());

            // see mimir docs on this error condition. it's not actionable from the proxy
            // so we drop it.
            if body.contains("err-mimir-sample-out-of-order") {
                CONSUMER_OPS
                    .with_label_values(&["check_response", "BAD_REQUEST"])
                    .inc();
                error!("({}) ERROR: {:?}", reqwest::StatusCode::BAD_REQUEST, body);
                return Err((
                    StatusCode::INTERNAL_SERVER_ERROR,
                    "IGNORING METRICS due to err-mimir-sample-out-of-order",
                ));
            }
            CONSUMER_OPS
                .with_label_values(&["check_response", "INTERNAL_SERVER_ERROR"])
                .inc();
            error!("({}) ERROR: {:?}", reqwest::StatusCode::BAD_REQUEST, body);
            Err((
                StatusCode::INTERNAL_SERVER_ERROR,
                "unknown bad request error encountered in remote_push",
            ))
        }
        code => {
            error!("TRIED: {:?}", request);
            let body = response
                .text()
                .await
                .unwrap_or_else(|_| "response body cannot be decoded".into());
            CONSUMER_OPS
                .with_label_values(&["check_response", "INTERNAL_SERVER_ERROR"])
                .inc();
            error!("({}) ERROR: {:?}", code, body);
            Err((
                StatusCode::INTERNAL_SERVER_ERROR,
                "unknown error encountered in remote_push",
            ))
        }
    }
}

pub async fn convert_to_remote_write(
    rc: ReqwestClient,
    node_metric: NodeMetric,
) -> (StatusCode, &'static str) {
    let timer = CONSUMER_OPERATION_DURATION
        .with_label_values(&["convert_to_remote_write"])
        .start_timer();
    // a counter so we don't iterate the node data 2x
    let mut mf_cnt = 0;
    for request in Mimir::from(node_metric.data) {
        mf_cnt += 1;
        let compressed = match encode_compress(&request) {
            Ok(compressed) => compressed,
            Err(error) => return error,
        };
        let response = match rc
            .client
            .post(rc.settings.url.to_owned())
            .header(reqwest::header::CONTENT_ENCODING, "snappy")
            .header(reqwest::header::CONTENT_TYPE, "application/x-protobuf")
            .header("X-Prometheus-Remote-Write-Version", "0.1.0")
            .basic_auth(
                rc.settings.username.to_owned(),
                Some(rc.settings.password.to_owned()),
            )
            .body(compressed)
            .send()
            .await
        {
            Ok(response) => response,
            Err(error) => {
                CONSUMER_OPS
                    .with_label_values(&["check_response", "INTERNAL_SERVER_ERROR"])
                    .inc();
                error!("DROPPING METRICS due to post error: {error}");
                timer.observe_duration();
                return (
                    StatusCode::INTERNAL_SERVER_ERROR,
                    "DROPPING METRICS due to post error",
                );
            }
        };
        match check_response(request, response).await {
            Ok(_) => (),
            Err(error) => {
                timer.observe_duration();
                return error;
            }
        }
    }
    CONSUMER_OPS_SUBMITTED.inc_by(mf_cnt as f64);
    timer.observe_duration();
    (StatusCode::CREATED, "created")
}

#[cfg(test)]
mod tests {
    use prometheus::proto;
    use protobuf;

    use crate::{
        consumer::populate_labels,
        prom_to_mimir::tests::{
            create_histogram, create_labels, create_metric_family, create_metric_histogram,
        },
    };

    #[test]
    fn test_populate_labels() {
        let mf = create_metric_family(
            "test_histogram",
            "i'm a help message",
            Some(proto::MetricType::HISTOGRAM),
            protobuf::RepeatedField::from(vec![create_metric_histogram(
                protobuf::RepeatedField::from_vec(create_labels(vec![])),
                create_histogram(),
            )]),
        );

        let labeled_mf = populate_labels(
            "validator-0".into(),
            "unittest-network".into(),
            "inventory-hostname".into(),
            vec![mf],
        );
        let metric = &labeled_mf[0].get_metric()[0];
        assert_eq!(
            metric.get_label(),
            &create_labels(vec![
                ("network", "unittest-network"),
                ("host", "validator-0"),
                ("relay_host", "inventory-hostname"),
            ])
        );
    }
}
