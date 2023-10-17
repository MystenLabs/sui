// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

#[cfg(feature = "pg_integration")]
mod tests {
    use sui_graphql_rpc::config::ConnectionConfig;

    #[tokio::test]
    async fn test_simple_client() {
        let connection_config = ConnectionConfig::ci_integration_test_cfg();

        let cluster = sui_graphql_rpc::cluster::start_cluster(connection_config).await;

        // Wait for servers to start and catchup
        tokio::time::sleep(std::time::Duration::from_secs(10)).await;

        let query = r#"
            query {
                chainIdentifier
            }
        "#;
        let res = cluster
            .graphql_client
            .execute(query.to_string(), vec![])
            .await
            .unwrap();
        let chain_id_actual = cluster
            .validator_fullnode_handle
            .fullnode_handle
            .sui_client
            .read_api()
            .get_chain_identifier()
            .await
            .unwrap();

        let exp = format!(
            "{{\"data\":{{\"chainIdentifier\":\"{}\"}}}}",
            chain_id_actual
        );
        assert_eq!(&format!("{}", res), &exp);
    }
}
