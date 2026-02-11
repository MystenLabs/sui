// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

//! Chrome Trace Viewer format conversion utilities.

use crate::{TimestampedEvent, TxEventType};
use serde::Serialize;
use serde_json::json;
use std::collections::HashMap;

/// Chrome Trace Event format
#[derive(Debug, Serialize, Clone)]
pub struct ChromeTraceEvent {
    pub name: String,
    pub cat: String,
    pub ph: String, // Phase: "B" (begin), "E" (end), or "X" (complete)
    pub ts: i64,    // Timestamp in microseconds
    #[serde(skip_serializing_if = "Option::is_none")]
    pub dur: Option<i64>, // Duration in microseconds (for "X" events)
    pub pid: u32,   // Process ID
    pub tid: String, // Thread ID (object ID)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub args: Option<serde_json::Value>,
}

/// Transaction data with input objects
#[derive(Debug, Clone)]
pub struct TransactionData {
    pub input_objects: Vec<String>,
}

/// Convert trace events to Chrome Trace format
///
/// Takes a list of timestamped events and a map of transaction digests to their input objects,
/// and produces Chrome Trace events suitable for visualization in chrome://tracing.
///
/// Each input object is mapped to a separate "thread" (tid) in the Chrome Trace format,
/// allowing visualization of object utilization over time.
pub fn convert_to_chrome_trace(
    events: Vec<TimestampedEvent>,
    tx_data_map: HashMap<String, TransactionData>,
) -> Vec<ChromeTraceEvent> {
    let mut chrome_events = Vec::new();
    let mut tx_begin_times: HashMap<String, i64> = HashMap::new();

    for event in events {
        let digest_hex = hex::encode(event.digest);
        let digest_base58 = bs58::encode(event.digest).into_string();

        match event.event_type {
            TxEventType::Enqueued | TxEventType::Scheduled => {
                // These events are not visualized in Chrome Trace format
            }
            TxEventType::ExecutionBegin => {
                // Convert SystemTime to microseconds since epoch
                let ts = event
                    .timestamp
                    .duration_since(std::time::UNIX_EPOCH)
                    .unwrap()
                    .as_micros() as i64;

                tx_begin_times.insert(digest_hex.clone(), ts);

                // Get input objects for this transaction (map key is hex)
                if let Some(tx_data) = tx_data_map.get(&digest_hex) {
                    for object_id in &tx_data.input_objects {
                        // Create a Begin event for each object
                        chrome_events.push(ChromeTraceEvent {
                            name: digest_base58.clone(),
                            cat: "transaction".to_string(),
                            ph: "B".to_string(),
                            ts,
                            dur: None,
                            pid: 1,
                            tid: object_id.clone(),
                            args: Some(json!({
                                "digest": &digest_base58,
                                "object": object_id,
                            })),
                        });
                    }
                }
            }
            TxEventType::ExecutionComplete => {
                let ts = event
                    .timestamp
                    .duration_since(std::time::UNIX_EPOCH)
                    .unwrap()
                    .as_micros() as i64;

                // Get input objects for this transaction (map key is hex)
                if let Some(tx_data) = tx_data_map.get(&digest_hex) {
                    for object_id in &tx_data.input_objects {
                        // Create an End event for each object
                        chrome_events.push(ChromeTraceEvent {
                            name: digest_base58.clone(),
                            cat: "transaction".to_string(),
                            ph: "E".to_string(),
                            ts,
                            dur: None,
                            pid: 1,
                            tid: object_id.clone(),
                            args: Some(json!({
                                "digest": &digest_base58,
                                "object": object_id,
                                "duration_us": ts - tx_begin_times.get(&digest_hex).unwrap_or(&ts),
                            })),
                        });
                    }
                }
            }
        }
    }

    chrome_events
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::UNIX_EPOCH;

    #[test]
    fn test_convert_to_chrome_trace() {
        // Create test events with known digests
        let tx1_digest = [1u8; 32];
        let tx2_digest = [2u8; 32];

        let base_time = UNIX_EPOCH + std::time::Duration::from_secs(1000000);

        let events = vec![
            TimestampedEvent {
                digest: tx1_digest,
                event_type: TxEventType::ExecutionBegin,
                timestamp: base_time,
            },
            TimestampedEvent {
                digest: tx2_digest,
                event_type: TxEventType::ExecutionBegin,
                timestamp: base_time + std::time::Duration::from_millis(50),
            },
            TimestampedEvent {
                digest: tx1_digest,
                event_type: TxEventType::ExecutionComplete,
                timestamp: base_time + std::time::Duration::from_millis(100),
            },
            TimestampedEvent {
                digest: tx2_digest,
                event_type: TxEventType::ExecutionComplete,
                timestamp: base_time + std::time::Duration::from_millis(150),
            },
        ];

        // Create transaction data mapping
        let mut tx_data_map = HashMap::new();
        tx_data_map.insert(
            hex::encode(tx1_digest),
            TransactionData {
                input_objects: vec![
                    "0x000000000000000000000000000000000000000000000000000000000000000a"
                        .to_string(),
                    "0x000000000000000000000000000000000000000000000000000000000000000b"
                        .to_string(),
                ],
            },
        );
        tx_data_map.insert(
            hex::encode(tx2_digest),
            TransactionData {
                input_objects: vec![
                    "0x000000000000000000000000000000000000000000000000000000000000000b"
                        .to_string(),
                    "0x000000000000000000000000000000000000000000000000000000000000000c"
                        .to_string(),
                ],
            },
        );

        // Convert to Chrome Trace format
        let chrome_events = convert_to_chrome_trace(events, tx_data_map);

        // Should have 8 events total (2 objects × 2 events × 2 transactions)
        assert_eq!(chrome_events.len(), 8);

        // Snapshot test the output
        insta::assert_json_snapshot!(chrome_events, {
            ".**.ts" => "[timestamp]",
            ".**.duration_us" => "[duration]",
        });
    }
}
