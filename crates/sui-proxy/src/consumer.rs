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
use prometheus::proto::{self, MetricFamily};
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
            1.0, 1.25, 1.5, 1.75, 2.0, 2.25, 2.5, 2.75, 3.0, 3.25, 3.5, 3.75, 4.0, 4.25, 4.5, 4.75,
            5.0, 5.25, 5.5, 5.75, 6.0, 6.25, 6.5, 6.75, 7.0, 7.25, 7.5, 7.75, 8.0, 8.25, 8.5, 8.75,
            9.0, 9.25, 9.5, 9.75, 10.0, 10.25, 10.5, 10.75, 11.0, 11.25, 11.5, 11.75, 12.0, 12.25,
            12.5, 12.75, 13.0, 13.25, 13.5, 13.75, 14.0, 14.25, 14.5, 14.75, 15.0, 15.25, 15.5,
            15.75, 16.0, 16.25, 16.5, 16.75, 17.0, 17.25, 17.5, 17.75, 18.0, 18.25, 18.5, 18.75,
            19.0, 19.25, 19.5, 19.75, 20.0, 20.25, 20.5, 20.75, 21.0, 21.25, 21.5, 21.75, 22.0,
            22.25, 22.5, 22.75, 23.0, 23.25, 23.5, 23.75, 24.0, 24.25, 24.5, 24.75, 25.0, 26.0,
            27.0, 28.0, 29.0, 30.0
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
    debug!("received metrics from {name} on {inventory_hostname}");
    // proto::LabelPair doesn't have pub fields so we can't use
    // struct literals to construct
    let mut network_label = proto::LabelPair::default();
    network_label.set_name("network".into());
    network_label.set_value(network);

    let mut host_label = proto::LabelPair::default();
    host_label.set_name("host".into());
    host_label.set_value(name);

    let labels = vec![network_label, host_label];

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
        || {
            timer.observe_duration();
        }
    }();
    let mut buf = Vec::with_capacity(request.encoded_len());
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

async fn convert(
    mfs: Vec<MetricFamily>,
) -> Result<impl Iterator<Item = WriteRequest>, (StatusCode, &'static str)> {
    let result = tokio::task::spawn_blocking(|| {
        let timer = CONSUMER_OPERATION_DURATION
            .with_label_values(&["convert_to_remote_write_task"])
            .start_timer();
        let result = Mimir::from(mfs);
        timer.observe_duration();
        result.into_iter()
    })
    .await;

    let result = match result {
        Ok(v) => v,
        Err(err) => {
            error!("unable to convert to remote_write; {err}");
            return Err((
                StatusCode::INTERNAL_SERVER_ERROR,
                "DROPPING METRICS; unable to convert to remote_write",
            ));
        }
    };
    Ok(result)
}

/// convert_to_remote_write is an expensive method due to the time it takes to submit to mimir.
/// other operations here are optimized for async, within reason.  The post process uses a single
/// connection to mimir and thus incurs the seriliaztion delay for each metric family sent. Possible
/// future optimizations would be to use multiple tcp connections to mimir, within reason. Nevertheless
/// we await on each post of each metric family so it shouldn't block any other async work in a
/// significant way.
pub async fn convert_to_remote_write(
    rc: ReqwestClient,
    node_metric: NodeMetric,
) -> (StatusCode, &'static str) {
    let timer = CONSUMER_OPERATION_DURATION
        .with_label_values(&["convert_to_remote_write"])
        .start_timer();

    let remote_write_protos = match convert(node_metric.data).await {
        Ok(v) => v,
        Err(err) => {
            timer.stop_and_discard();
            return err;
        }
    };

    // a counter so we don't iterate the node data 2x
    let mut mf_cnt = 0;
    for request in remote_write_protos {
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
                timer.stop_and_discard();
                return (
                    StatusCode::INTERNAL_SERVER_ERROR,
                    "DROPPING METRICS due to post error",
                );
            }
        };

        match check_response(request, response).await {
            Ok(_) => (),
            Err(err) => {
                timer.stop_and_discard();
                return err;
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
            ])
        );
    }
}
