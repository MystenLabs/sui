// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

//! Demo client for streaming liquidity pool updates via WebSocket.
//!
//! This example demonstrates how to connect to the custom broadcaster
//! and subscribe to liquidity pool updates from various DEX protocols.
//!
//! # Running the demo
//!
//! First, ensure a Sui node with the custom broadcaster is running on port 9003.
//! Then run this example:
//!
//! ```bash
//! cargo run --example liquidity_stream_demo
//! ```
//!
//! # Protocol
//!
//! The WebSocket protocol uses JSON messages:
//!
//! ## Subscribe to all pools:
//! ```json
//! {"action": "subscribe_liquidity", "protocols": [], "pool_ids": []}
//! ```
//!
//! ## Subscribe to specific protocols (regex supported):
//! ```json
//! {"action": "subscribe_liquidity", "protocols": ["cetus", "turbos"]}
//! ```
//!
//! ## Subscribe to specific pool IDs:
//! ```json
//! {"action": "subscribe_liquidity", "pool_ids": ["0x..."]}
//! ```
//!
//! ## Unsubscribe:
//! ```json
//! {"action": "unsubscribe"}
//! ```

use futures::{SinkExt, StreamExt};
use serde::{Deserialize, Serialize};
use std::env;
use tokio_tungstenite::{connect_async, tungstenite::Message};

/// Message types received from the server
#[derive(Debug, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
enum StreamMessage {
    LiquidityUpdate(LiquidityUpdate),
    Subscribed(SubscriptionConfirmation),
    Error(ErrorMessage),
    Heartbeat { timestamp_ms: u64 },
}

#[derive(Debug, Deserialize)]
struct LiquidityUpdate {
    pool_id: String,
    protocol: String,
    pool_type: String,
    token_types: Vec<String>,
    digest: String,
    version: u64,
    #[serde(skip_serializing_if = "Option::is_none")]
    raw_bytes: Option<String>,
}

#[derive(Debug, Deserialize)]
struct SubscriptionConfirmation {
    subscription_id: String,
    protocols: Vec<String>,
    pool_ids: Vec<String>,
}

#[derive(Debug, Deserialize)]
struct ErrorMessage {
    code: String,
    message: String,
}

/// Client message types
#[derive(Debug, Serialize)]
#[serde(tag = "action", rename_all = "snake_case")]
enum ClientMessage {
    SubscribeLiquidity(LiquiditySubscription),
    Unsubscribe,
    Ping,
}

#[derive(Debug, Serialize)]
struct LiquiditySubscription {
    protocols: Vec<String>,
    pool_ids: Vec<String>,
    include_raw_bytes: bool,
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    // Parse command line arguments
    let args: Vec<String> = env::args().collect();

    let url = if args.len() > 1 {
        args[1].clone()
    } else {
        "ws://localhost:9003/ws".to_string()
    };

    let protocols: Vec<String> = if args.len() > 2 {
        args[2].split(',').map(|s| s.trim().to_string()).collect()
    } else {
        vec![] // Subscribe to all protocols
    };

    println!("Connecting to {}...", url);
    println!("Subscribing to protocols: {:?}", if protocols.is_empty() { vec!["all".to_string()] } else { protocols.clone() });

    // Connect to WebSocket
    let (ws_stream, _) = connect_async(&url).await?;
    println!("Connected!");

    let (mut write, mut read) = ws_stream.split();

    // Send subscription request
    let subscribe_msg = ClientMessage::SubscribeLiquidity(LiquiditySubscription {
        protocols,
        pool_ids: vec![],
        include_raw_bytes: false,
    });

    let json = serde_json::to_string(&subscribe_msg)?;
    write.send(Message::Text(json.into())).await?;
    println!("Subscription request sent\n");

    // Listen for messages
    let mut update_count = 0u64;
    while let Some(msg) = read.next().await {
        match msg {
            Ok(Message::Text(text)) => {
                match serde_json::from_str::<StreamMessage>(&text) {
                    Ok(StreamMessage::LiquidityUpdate(update)) => {
                        update_count += 1;
                        println!("=== Liquidity Update #{} ===", update_count);
                        println!("Pool ID:     {}", update.pool_id);
                        println!("Protocol:    {}", update.protocol);
                        println!("Pool Type:   {}", update.pool_type);
                        println!("Tokens:      {:?}", update.token_types);
                        println!("Transaction: {}", update.digest);
                        println!("Version:     {}", update.version);
                        if let Some(ref bytes) = update.raw_bytes {
                            println!("Raw Bytes:   {} bytes (base64)", bytes.len());
                        }
                        println!();
                    }
                    Ok(StreamMessage::Subscribed(confirmation)) => {
                        println!("=== Subscription Confirmed ===");
                        println!("Subscription ID: {}", confirmation.subscription_id);
                        println!("Protocols:       {:?}", confirmation.protocols);
                        println!("Pool IDs:        {:?}", confirmation.pool_ids);
                        println!();
                    }
                    Ok(StreamMessage::Error(error)) => {
                        eprintln!("Error [{}]: {}", error.code, error.message);
                    }
                    Ok(StreamMessage::Heartbeat { timestamp_ms }) => {
                        println!("Heartbeat: {}ms", timestamp_ms);
                    }
                    Err(e) => {
                        eprintln!("Failed to parse message: {}", e);
                        eprintln!("Raw message: {}", text);
                    }
                }
            }
            Ok(Message::Close(frame)) => {
                println!("Connection closed: {:?}", frame);
                break;
            }
            Ok(Message::Ping(_)) => {
                // Pong is automatically sent
            }
            Ok(_) => {}
            Err(e) => {
                eprintln!("WebSocket error: {}", e);
                break;
            }
        }
    }

    println!("Total updates received: {}", update_count);
    Ok(())
}
