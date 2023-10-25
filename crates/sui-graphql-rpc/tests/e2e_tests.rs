// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

#[cfg(feature = "pg_integration")]
mod tests {
    use diesel::OptionalExtension;
    use diesel::RunQueryDsl;
    use diesel::{ExpressionMethods, QueryDsl};
    use rand::rngs::StdRng;
    use rand::SeedableRng;
    use serial_test::serial;
    use simulacrum::Simulacrum;
    use std::sync::Arc;
    use std::time::Duration;
    use sui_graphql_rpc::config::ConnectionConfig;
    use sui_graphql_rpc::context_data::db_query_cost::extract_cost;
    use sui_indexer::indexer_reader::IndexerReader;
    use sui_indexer::models_v2::objects::StoredObject;
    use sui_indexer::new_pg_connection_pool_impl;
    use sui_indexer::schema_v2::objects;
    use sui_indexer::utils::reset_database;
    use sui_indexer::PgConnectionPoolConfig;
    use sui_types::digests::ChainIdentifier;
    use tokio::time::sleep;

    #[tokio::test]
    #[serial]
    async fn test_simple_client_validator_cluster() {
        let _guard = telemetry_subscribers::TelemetryConfig::new()
            .with_env()
            .init();

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
    #[serial]
    async fn test_simple_client_simulator_cluster() {
        sleep(Duration::from_secs(5)).await;
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

    #[tokio::test]
    #[serial]
    async fn test_db_query_cost() {
        // Wait for DB to free up
        sleep(Duration::from_secs(5)).await;
        let connection_config = ConnectionConfig::ci_integration_test_cfg();
        let parsed_url = connection_config.db_url();
        let blocking_pool = new_pg_connection_pool_impl(&parsed_url, Some(2)).unwrap();
        reset_database(&mut blocking_pool.get().unwrap(), true, true).unwrap();

        // Test query cost logic
        let mut query = objects::dsl::objects.into_boxed();
        query = query
            .filter(objects::dsl::object_id.eq(vec![0u8, 4]))
            .filter(objects::dsl::object_version.eq(1234i64));

        let mut idx_cfg = PgConnectionPoolConfig::default();
        idx_cfg.set_pool_size(20);
        let reader = IndexerReader::new_with_config(connection_config.db_url(), idx_cfg).unwrap();
        reader
            .run_query_async(|conn| {
                let cost = extract_cost(&query, conn).unwrap();
                assert!(cost > 0.0);
                query.get_result::<StoredObject>(conn).optional()
            })
            .await
            .unwrap();
    }

    use sui_graphql_rpc::server::builder::tests::*;

    #[tokio::test]
    #[serial]
    async fn test_timeout() {
        test_timeout_impl().await;
    }

    #[tokio::test]
    #[serial]
    async fn test_query_depth_limit() {
        test_query_depth_limit_impl().await;
    }

    #[tokio::test]
    #[serial]
    async fn test_query_node_limit() {
        test_query_node_limit_impl().await;
    }

    #[tokio::test]
    #[serial]
    async fn test_query_complexity_metrics() {
        test_query_complexity_metrics_impl().await;
    }
}
