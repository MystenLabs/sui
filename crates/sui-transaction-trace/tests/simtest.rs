// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

#[cfg(msim)]
mod test {
    use std::collections::HashMap;
    use std::path::PathBuf;
    use sui_macros::sim_test;
    use sui_swarm_config::network_config_builder::ConfigBuilder;
    use sui_types::base_types::SuiAddress;
    use sui_types::crypto::{AccountKeyPair, get_key_pair};
    use sui_types::digests::TransactionDigest;
    use sui_types::programmable_transaction_builder::ProgrammableTransactionBuilder;
    use sui_types::quorum_driver_types::ExecuteTransactionRequestType;
    use sui_types::transaction::{
        CallArg, ObjectArg, TEST_ONLY_GAS_UNIT_FOR_TRANSFER, Transaction,
    };
    use test_cluster::TestClusterBuilder;
    use tokio::time::{Duration, sleep};

    #[sim_test]
    async fn test_transaction_trace_e2e() {
        sui_protocol_config::ProtocolConfig::poison_get_for_min_version();

        // Create a temporary directory for trace logs
        let tmpdir = tempfile::tempdir().unwrap();
        let trace_log_dir = tmpdir.path().join("transaction-traces");

        // Create network config with transaction trace logging enabled
        let mut network_config = ConfigBuilder::new_with_temp_dir()
            .with_num_validators(4)
            .build();

        // Enable transaction trace logging for all validators
        for validator_config in network_config.validator_configs_mut() {
            validator_config.transaction_trace_config =
                Some(sui_config::node::TransactionTraceConfig {
                    log_dir: Some(trace_log_dir.clone()),
                    max_file_size: Some(10 * 1024 * 1024), // 10MB
                    max_file_count: Some(5),
                    buffer_capacity: Some(1000),
                    flush_interval_secs: Some(1), // Fast flush for testing
                });
        }

        // Build test cluster with transaction tracing enabled
        let test_cluster = TestClusterBuilder::new()
            .set_network_config(network_config)
            .build()
            .await;

        // Get wallet and context for executing transactions
        let context = &test_cluster.wallet;
        let mut client = test_cluster.grpc_client();
        let sender = context.config.active_address.unwrap();
        let keystore = &context.config.keystore;

        // Execute some transactions to generate trace data
        let gas_objects = context
            .get_all_gas_objects_owned_by_address(sender)
            .await
            .unwrap();

        let recipient: SuiAddress = get_key_pair::<AccountKeyPair>().0.public().into();
        let mut tx_digests = Vec::new();
        let mut tx_gas_objects = Vec::new();

        // Execute 5 transfer transactions
        for i in 0..5 {
            if let Some(gas) = gas_objects.get(i) {
                let gas_ref = gas.compute_object_reference();
                let gas_id = gas_ref.0;

                // Build transfer transaction
                let mut builder = ProgrammableTransactionBuilder::new();
                let amount = 1000 * (i as u64 + 1);
                builder.transfer_sui(recipient, Some(amount));

                let pt = builder.finish();
                let gas_price = context.get_reference_gas_price().await.unwrap();

                let data = sui_types::transaction::TransactionData::new_programmable(
                    sender,
                    vec![gas_ref],
                    pt,
                    TEST_ONLY_GAS_UNIT_FOR_TRANSFER * gas_price,
                    gas_price,
                );

                let tx = context.sign_transaction(&data);

                // Execute transaction
                let response = context.execute_transaction_must_succeed(tx).await;

                tx_digests.push(*response.digest());
                tx_gas_objects.push(gas_id);
                println!("Executed transaction {}: {}", i + 1, response.digest());
            }
        }

        // Wait for trace logs to be flushed
        sleep(Duration::from_secs(2)).await;

        // Read trace logs from the directory
        println!("\nReading trace logs from: {}", trace_log_dir.display());
        let trace_files = std::fs::read_dir(&trace_log_dir)
            .unwrap()
            .filter_map(|entry| entry.ok())
            .filter(|entry| entry.path().extension().and_then(|s| s.to_str()) == Some("bin"))
            .map(|entry| entry.path())
            .collect::<Vec<_>>();

        println!("Found {} trace log files", trace_files.len());
        assert!(!trace_files.is_empty(), "No trace log files found");

        // Read all events from trace logs
        let mut all_events = Vec::new();
        for trace_file in &trace_files {
            println!("Reading trace file: {}", trace_file.display());
            let reader = sui_transaction_trace::LogReader::new(trace_file).unwrap();
            for event_result in reader {
                let event = event_result.unwrap();
                all_events.push(event);
            }
        }

        println!("\nRead {} total trace events", all_events.len());
        assert!(!all_events.is_empty(), "No trace events found");

        // Build transaction data map using gas object IDs we know from transaction execution
        let mut tx_data_map = HashMap::new();
        for (i, digest) in tx_digests.iter().enumerate() {
            let digest_bytes: [u8; 32] = digest.into_inner();

            // Find events for this transaction
            let has_events = all_events.iter().any(|e| e.digest == digest_bytes);

            if has_events {
                let gas_id = tx_gas_objects[i];
                let input_objects = vec![format!("0x{}", hex::encode(gas_id))];

                println!(
                    "Transaction {} has {} input objects",
                    bs58::encode(digest_bytes).into_string(),
                    input_objects.len()
                );

                tx_data_map.insert(
                    hex::encode(digest_bytes),
                    sui_transaction_trace::chrome_trace::TransactionData { input_objects },
                );
            }
        }

        // Convert to Chrome trace format
        println!("\nConverting to Chrome trace format...");
        let chrome_events =
            sui_transaction_trace::chrome_trace::convert_to_chrome_trace(&all_events, &tx_data_map);

        println!("Generated {} Chrome trace events", chrome_events.len());
        assert!(
            !chrome_events.is_empty(),
            "No Chrome trace events generated"
        );

        // Write Chrome trace output to tmpdir
        let chrome_trace_output = tmpdir.path().join("trace.json");
        let chrome_trace_json = serde_json::json!({
            "traceEvents": chrome_events,
            "displayTimeUnit": "ms",
        });

        std::fs::write(
            &chrome_trace_output,
            serde_json::to_string_pretty(&chrome_trace_json).unwrap(),
        )
        .unwrap();

        println!("\n===========================================");
        println!("Chrome trace written to:");
        println!("{}", chrome_trace_output.display());
        println!("===========================================");
        println!("\nTo view: Open chrome://tracing and load the file");

        // Verify the output file exists and is valid JSON
        let output_content = std::fs::read_to_string(&chrome_trace_output).unwrap();
        let parsed: serde_json::Value = serde_json::from_str(&output_content).unwrap();
        assert!(parsed["traceEvents"].is_array());
        assert!(parsed["traceEvents"].as_array().unwrap().len() > 0);
    }
}
