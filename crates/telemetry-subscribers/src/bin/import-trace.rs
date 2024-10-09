// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use bytes::Buf;
use bytes_varint::VarIntSupport;
use clap::*;
use opentelemetry_proto::tonic::{
    collector::trace::v1::{trace_service_client::TraceServiceClient, ExportTraceServiceRequest},
    common::v1::{any_value, AnyValue, KeyValue},
};
use prost::Message;
use std::io::{self, Cursor, Read};
use tonic::Request;

#[derive(Parser, Debug)]
#[command(author, version, about, long_about = None)]
struct Args {
    #[arg(long)]
    trace_file: String,

    #[arg(long, default_value = "http://localhost:4317")]
    otlp_endpoint: String,

    #[arg(long)]
    service_name: Option<String>,

    #[arg(long)]
    dump_spans: bool,
}

#[tokio::main]
async fn main() {
    let args = Args::parse();
    let file = std::fs::File::open(args.trace_file).unwrap();

    let messages = decode_all_length_delimited::<_, ExportTraceServiceRequest>(file).unwrap();

    if args.dump_spans {
        for message in messages.iter() {
            for span in &message.resource_spans {
                println!("{:#?}", span);
            }
        }
        return;
    }

    let endpoint = format!("{}{}", args.otlp_endpoint, "/v1/traces");
    let mut trace_exporter = TraceServiceClient::connect(endpoint).await.unwrap();

    let service_name = args.service_name.unwrap_or_else(|| {
        let timestamp = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_secs();
        format!("sui-node-{}", timestamp)
    });

    println!("importing trace with service name {:?}", service_name);

    for mut message in messages {
        let mut span_count = 0;

        // Rewrite the service name to separate the imported trace from other traces
        for resource_span in message.resource_spans.iter_mut() {
            for scope_span in resource_span.scope_spans.iter() {
                span_count += scope_span.spans.len();
            }

            if let Some(resource) = resource_span.resource.as_mut() {
                let mut service_name_found = false;
                for attr in resource.attributes.iter_mut() {
                    if attr.key == "service.name" {
                        service_name_found = true;
                        attr.value = Some(AnyValue {
                            value: Some(any_value::Value::StringValue(service_name.clone())),
                        });
                    }
                }
                if !service_name_found {
                    resource.attributes.push(KeyValue {
                        key: "service.name".to_string(),
                        value: Some(AnyValue {
                            value: Some(any_value::Value::StringValue(service_name.clone())),
                        }),
                    });
                }
            }
        }

        println!("sending {} spans to otlp collector", span_count);
        trace_exporter.export(Request::new(message)).await.unwrap();
    }
    println!("all spans imported");
}

fn decode_all_length_delimited<R, M>(mut reader: R) -> io::Result<Vec<M>>
where
    R: Read,
    M: Message + Default,
{
    let mut messages = Vec::new();
    let mut buffer = Vec::new();
    reader.read_to_end(&mut buffer)?;
    let mut cursor = Cursor::new(buffer);

    while cursor.has_remaining() {
        let len = cursor.get_u64_varint().unwrap() as usize;

        if cursor.remaining() < len {
            return Err(io::Error::new(
                io::ErrorKind::UnexpectedEof,
                "Incomplete message",
            ));
        }

        // Create a slice for just this message
        let msg_bytes = cursor
            .chunk()
            .get(..len)
            .ok_or_else(|| io::Error::new(io::ErrorKind::UnexpectedEof, "Buffer underflow"))?;

        let msg = M::decode(msg_bytes).map_err(|e| {
            io::Error::new(io::ErrorKind::InvalidData, format!("Decode error: {}", e))
        })?;
        messages.push(msg);

        // Advance the cursor
        cursor.advance(len);
    }

    Ok(messages)
}
