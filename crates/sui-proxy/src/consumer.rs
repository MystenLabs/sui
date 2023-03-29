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
use prometheus::proto::{self, MetricFamily};
use prost::Message;
use protobuf::CodedInputStream;
use std::io::Read;
use tracing::{debug, error};

/// NodeMetric holds metadata and a metric payload from the calling node
#[derive(Debug)]
pub struct NodeMetric {
    pub host: String,                 // the sui node name from the blockchain
    pub network: String,              // the sui blockchain name, mainnet
    pub peer_addr: Multiaddr,         // the sockaddr source address from the incoming request
    pub public_key: Ed25519PublicKey, // the public key from the sui blockchain
    pub data: Vec<MetricFamily>,      // decoded protobuf of prometheus data
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
        Ok(result)
    }
}

// populate labels in place for our given metric family data
fn populate_labels(node_metric: NodeMetric) -> Vec<MetricFamily> {
    // proto::LabelPair doesn't have pub fields so we can't use
    // struct literals to construct
    let mut network_label = proto::LabelPair::default();
    network_label.set_name("network".into());
    network_label.set_value(node_metric.network);

    let mut host_label = proto::LabelPair::default();
    host_label.set_name("host".into());
    host_label.set_value(node_metric.host);

    let labels = vec![network_label, host_label];

    let mut data = node_metric.data;
    // add our extra labels to our incoming metric data
    for mf in data.iter_mut() {
        for m in mf.mut_metric() {
            m.mut_label().extend(labels.clone());
        }
    }
    data
}

fn encode_compress(request: &WriteRequest) -> Result<Vec<u8>, (StatusCode, &'static str)> {
    let mut buf = Vec::new();
    buf.reserve(request.encoded_len());
    let Ok(()) = request.encode(&mut buf) else {
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
            error!("unable to compress to snappy block format; {error}");
            return Err((
                StatusCode::INTERNAL_SERVER_ERROR,
                "unable to compress to snappy block format",
            ));
        }
    };
    Ok(compressed)
}

async fn check_response(
    request: WriteRequest,
    response: reqwest::Response,
) -> Result<(), (StatusCode, &'static str)> {
    match response.status() {
        reqwest::StatusCode::OK => {
            debug!("({}) SUCCESS: {:?}", reqwest::StatusCode::OK, request);
            Ok(())
        }
        reqwest::StatusCode::BAD_REQUEST => {
            error!("TRIED: {:?}", request);
            let body = response
                .text()
                .await
                .unwrap_or_else(|_| "response body cannot be decoded".into());

            if body.contains("err-mimir-sample-out-of-order") {
                error!("({}) ERROR: {:?}", reqwest::StatusCode::BAD_REQUEST, body);
                return Err((
                    StatusCode::INTERNAL_SERVER_ERROR,
                    "IGNORNING METRICS due to err-mimir-sample-out-of-order",
                ));
            }
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
    let data = populate_labels(node_metric);
    for request in Mimir::from(data) {
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
                error!("DROPPING METRICS due to post error: {error}");
                return (
                    StatusCode::INTERNAL_SERVER_ERROR,
                    "DROPPING METRICS due to post error",
                );
            }
        };
        match check_response(request, response).await {
            Ok(_) => (),
            Err(error) => return error,
        }
    }
    (StatusCode::CREATED, "created")
}
