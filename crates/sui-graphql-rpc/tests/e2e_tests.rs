// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

#[cfg(feature = "pg_integration")]
mod tests {
    use sui_graphql_rpc::client::simple_client::SimpleClient;
    use sui_graphql_rpc::config::{ConnectionConfig, ServiceConfig};
    use sui_graphql_rpc::server::simple_server::start_example_server;
    use sui_graphql_rpc::utils::reset_db;

    #[tokio::test]
    async fn test_client() {
        let mut handles = vec![];
        let db_url = "postgres://postgres:postgrespw@localhost:5432/sui_indexer_v2".to_string();
        reset_db(&db_url, true, true).unwrap();
        let connection_config = ConnectionConfig::new(None, None, None, Some(db_url), None, None);

        handles.push(tokio::spawn(async move {
            start_example_server(connection_config, ServiceConfig::default()).await;
        }));

        // Wait for server to start
        tokio::time::sleep(std::time::Duration::from_secs(10)).await;
        let client = SimpleClient::new("http://127.0.0.1:8000/");
        let query = r#"
            query {
                chainIdentifier
            }
        "#;
        let res = client.execute(query.to_string(), vec![]).await.unwrap();
        let exp = r#"{"data":{"chainIdentifier":"4c78adac"}}"#;
        assert_eq!(&format!("{}", res), exp);
    }
}
