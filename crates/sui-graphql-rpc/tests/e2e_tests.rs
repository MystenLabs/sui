// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

#[cfg(feature = "pg_integration")]
mod tests {
    use sui_graphql_rpc::config::ConnectionConfig;

    #[ignore]
    #[tokio::test]
    async fn test_simple_client_validator_cluster() {
        let connection_config = ConnectionConfig::ci_integration_test_cfg();

        let cluster = sui_graphql_rpc::cluster::start_cluster(connection_config, None).await;

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

    #[tokio::test]
    async fn test_simple_client_simulator_cluster() {
        use rand::rngs::StdRng;
        use rand::SeedableRng;
        use simulacrum::Simulacrum;
        use std::sync::Arc;
        use sui_types::digests::ChainIdentifier;
        let rng = StdRng::from_seed([12; 32]);
        let mut sim = Simulacrum::new_with_rng(rng);

        sim.create_checkpoint();
        sim.create_checkpoint();

        let genesis_checkpoint_digest1 = sim
            .store()
            .get_checkpoint_by_sequence_number(0)
            .unwrap()
            .digest();

        let chain_id_actual = format!("{}", ChainIdentifier::from(*genesis_checkpoint_digest1));
        let exp = format!(
            "{{\"data\":{{\"chainIdentifier\":\"{}\"}}}}",
            chain_id_actual
        );
        let connection_config = ConnectionConfig::ci_integration_test_cfg();
        let cluster =
            sui_graphql_rpc::cluster::serve_simulator(connection_config, 3000, Arc::new(sim)).await;

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

        assert_eq!(&format!("{}", res), &exp);
    }
}
