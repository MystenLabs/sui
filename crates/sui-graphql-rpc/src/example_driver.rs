// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use crate::client::simple_client::SimpleClient;
use crate::config::{ConnectionConfig, ServiceConfig};
use crate::server::simple_server::start_example_server;
use std::collections::BTreeMap;
use std::{fs, io::Read, path::PathBuf};

const QUERY_EXT: &str = "graphql";
const EXPECTED_RESULT_EXT: &str = "exp";

/// Loop through all files in the examples directory which end in .graphql
/// and load the contents into strings.
pub async fn verify_examples_impl() {
    let mut buf: PathBuf = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    buf.push("examples");

    let mut queries = BTreeMap::new();
    let mut expected_results = BTreeMap::new();

    let config = ConnectionConfig {
        port: 8123,
        ..Default::default()
    };
    let server_url = config.server_url();

    tokio::spawn(async move {
        start_example_server(config, ServiceConfig::default()).await;
    });

    // Wait for server to start
    tokio::time::sleep(std::time::Duration::from_secs(10)).await;
    let client = SimpleClient::new(server_url);

    for entry in std::fs::read_dir(buf).unwrap() {
        let entry = entry.unwrap();
        let path = entry.path();

        for file in std::fs::read_dir(path).unwrap() {
            assert!(file.is_ok());
            let file = file.unwrap();
            assert!(file.path().extension().is_some());
            let ext = file
                .path()
                .extension()
                .unwrap()
                .to_str()
                .unwrap()
                .to_string();
            assert!(ext == QUERY_EXT || ext == EXPECTED_RESULT_EXT);

            let file_path = file.path();
            let query_name = file_path.file_stem().unwrap().to_str().unwrap().to_string();

            let mut contents = String::new();
            let mut fp = fs::File::open(file_path).unwrap();
            fp.read_to_string(&mut contents).unwrap();

            if ext == QUERY_EXT {
                assert!(!queries.contains_key(query_name.as_str()));
                queries.insert(query_name.clone(), contents.clone());
            } else if ext == EXPECTED_RESULT_EXT {
                assert!(!expected_results.contains_key(query_name.as_str()));
                expected_results.insert(query_name.clone(), contents.clone());
            }

            // Todo: run concurrent queries to speed up tests
            if queries.contains_key(query_name.as_str())
                && expected_results.contains_key(query_name.as_str())
            {
                let query = queries.remove(query_name.as_str()).unwrap();
                let expected_result = expected_results.remove(query_name.as_str()).unwrap();
                let expected_result: serde_json::Value =
                    serde_json::from_str(expected_result.as_str()).unwrap();

                let actual_result: serde_json::Value =
                    client.execute(query.to_string()).await.unwrap();
                assert!(
                    serde_json::to_string_pretty(&actual_result).unwrap()
                        == serde_json::to_string_pretty(&expected_result).unwrap(),
                    "Query {} failed. Actual esult: {}",
                    query_name,
                    actual_result
                );
            }
        }
    }

    // For queries which don't have expected results, we can just run them and make sure they don't
    // error
    for (query_name, query) in queries.iter() {
        let actual_result: serde_json::Value = client.execute(query.to_string()).await.unwrap();
        assert!(
            actual_result.get("errors").is_none(),
            "Query {} failed: {}",
            query_name,
            actual_result
        );
    }

    assert!(
        expected_results.is_empty(),
        "Cannot have expected results without queries: {:?}",
        expected_results
    );
}

#[cfg(test)]
mod test {
    use crate::example_driver::verify_examples_impl;
    #[tokio::test]
    async fn test_examples() {
        verify_examples_impl().await;
    }
}
