// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

#[cfg(feature = "pg_integration")]
mod tests {
    use diesel::OptionalExtension;
    use diesel::RunQueryDsl;
    use diesel::{ExpressionMethods, QueryDsl};
    use rand::rngs::StdRng;
    use rand::SeedableRng;
    use serde_json::json;
    use serial_test::serial;
    use simulacrum::Simulacrum;
    use std::sync::Arc;
    use std::time::Duration;
    use sui_graphql_rpc::client::simple_client::GraphqlQueryVariable;
    use sui_graphql_rpc::config::ConnectionConfig;
    use sui_graphql_rpc::context_data::db_query_cost::extract_cost;
    use sui_graphql_rpc::test_infra::cluster::DEFAULT_INTERNAL_DATA_SOURCE_PORT;
    use sui_indexer::indexer_reader::IndexerReader;
    use sui_indexer::models_v2::objects::StoredObject;
    use sui_indexer::new_pg_connection_pool_impl;
    use sui_indexer::schema_v2::objects;
    use sui_indexer::utils::reset_database;
    use sui_indexer::PgConnectionPoolConfig;
    use sui_types::digests::ChainIdentifier;
    use sui_types::DEEPBOOK_ADDRESS;
    use sui_types::SUI_FRAMEWORK_ADDRESS;
    use tokio::time::sleep;

    #[tokio::test]
    #[serial]
    async fn test_simple_client_validator_cluster() {
        let _guard = telemetry_subscribers::TelemetryConfig::new()
            .with_env()
            .init();

        let connection_config = ConnectionConfig::ci_integration_test_cfg();

        let cluster =
            sui_graphql_rpc::test_infra::cluster::start_cluster(connection_config, None).await;

        // Wait for servers to start and catchup
        tokio::time::sleep(std::time::Duration::from_secs(10)).await;

        let query = r#"
            {
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

        let genesis_checkpoint_digest1 = *sim
            .store()
            .get_checkpoint_by_sequence_number(0)
            .unwrap()
            .digest();

        let chain_id_actual = format!("{}", ChainIdentifier::from(genesis_checkpoint_digest1));
        let exp = format!(
            "{{\"data\":{{\"chainIdentifier\":\"{}\"}}}}",
            chain_id_actual
        );
        let connection_config = ConnectionConfig::ci_integration_test_cfg();
        let cluster = sui_graphql_rpc::test_infra::cluster::serve_executor(
            connection_config,
            DEFAULT_INTERNAL_DATA_SOURCE_PORT,
            Arc::new(sim),
        )
        .await;

        let query = r#"
            {
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
            .spawn_blocking(move |this| {
                let cost = extract_cost(&query, &this).unwrap();
                assert!(cost > 0.0);
                this.run_query(|conn| query.get_result::<StoredObject>(conn).optional())
            })
            .await
            .unwrap();
    }

    #[tokio::test]
    #[serial]
    async fn test_graphql_client_response() {
        sleep(Duration::from_secs(5)).await;
        let rng = StdRng::from_seed([12; 32]);
        let mut sim = Simulacrum::new_with_rng(rng);

        sim.create_checkpoint();
        sim.create_checkpoint();

        let connection_config = ConnectionConfig::ci_integration_test_cfg();
        let cluster = sui_graphql_rpc::test_infra::cluster::serve_executor(
            connection_config,
            DEFAULT_INTERNAL_DATA_SOURCE_PORT,
            Arc::new(sim),
        )
        .await;

        let query = r#"
            {
                chainIdentifier
            }
        "#;
        let res = cluster
            .graphql_client
            .execute_to_graphql(query.to_string(), true, vec![], vec![])
            .await
            .unwrap();

        assert_eq!(res.http_status().as_u16(), 200);
        assert_eq!(res.http_version(), hyper::Version::HTTP_11);
        assert!(res.graphql_version().unwrap().len() >= 5);
        assert!(res.errors().is_empty());

        let usage = res.usage().unwrap().unwrap();
        assert_eq!(*usage.get("nodes").unwrap(), 1);
        assert_eq!(*usage.get("depth").unwrap(), 1);
        assert_eq!(*usage.get("variables").unwrap(), 0);
        assert_eq!(*usage.get("fragments").unwrap(), 0);
    }

    #[tokio::test]
    #[serial]
    async fn test_graphql_client_variables() {
        sleep(Duration::from_secs(5)).await;
        let rng = StdRng::from_seed([12; 32]);
        let mut sim = Simulacrum::new_with_rng(rng);

        sim.create_checkpoint();
        sim.create_checkpoint();

        let connection_config = ConnectionConfig::ci_integration_test_cfg();
        let cluster = sui_graphql_rpc::test_infra::cluster::serve_executor(
            connection_config,
            DEFAULT_INTERNAL_DATA_SOURCE_PORT,
            Arc::new(sim),
        )
        .await;

        let query = r#"{obj1: object(address: $framework_addr) {location}
            obj2: object(address: $deepbook_addr) {location}}"#;
        let variables = vec![
            GraphqlQueryVariable {
                name: "framework_addr".to_string(),
                ty: "SuiAddress!".to_string(),
                value: json!("0x2"),
            },
            GraphqlQueryVariable {
                name: "deepbook_addr".to_string(),
                ty: "SuiAddress!".to_string(),
                value: json!("0xdee9"),
            },
        ];
        let res = cluster
            .graphql_client
            .execute_to_graphql(query.to_string(), true, variables, vec![])
            .await
            .unwrap();

        assert!(res.errors().is_empty());
        let data = res.response_body().data.clone().into_json().unwrap();
        data.get("obj1").unwrap().get("location").unwrap();
        assert_eq!(
            data.get("obj1")
                .unwrap()
                .get("location")
                .unwrap()
                .as_str()
                .unwrap(),
            SUI_FRAMEWORK_ADDRESS.to_canonical_string(true)
        );
        assert_eq!(
            data.get("obj2")
                .unwrap()
                .get("location")
                .unwrap()
                .as_str()
                .unwrap(),
            DEEPBOOK_ADDRESS.to_canonical_string(true)
        );

        let bad_variables = vec![
            GraphqlQueryVariable {
                name: "framework_addr".to_string(),
                ty: "SuiAddress!".to_string(),
                value: json!("0x2"),
            },
            GraphqlQueryVariable {
                name: "deepbook_addr".to_string(),
                ty: "SuiAddress!".to_string(),
                value: json!("0xdee9"),
            },
            GraphqlQueryVariable {
                name: "deepbook_addr".to_string(),
                ty: "SuiAddress!".to_string(),
                value: json!("0xdee96666666"),
            },
        ];
        let res = cluster
            .graphql_client
            .execute_to_graphql(query.to_string(), true, bad_variables, vec![])
            .await;

        assert!(res.is_err());

        let bad_variables = vec![
            GraphqlQueryVariable {
                name: "framework_addr".to_string(),
                ty: "SuiAddress!".to_string(),
                value: json!("0x2"),
            },
            GraphqlQueryVariable {
                name: "deepbook_addr".to_string(),
                ty: "SuiAddress!".to_string(),
                value: json!("0xdee9"),
            },
            GraphqlQueryVariable {
                name: "deepbook_addr".to_string(),
                ty: "SuiAddressP!".to_string(),
                value: json!("0xdee9"),
            },
        ];
        let res = cluster
            .graphql_client
            .execute_to_graphql(query.to_string(), true, bad_variables, vec![])
            .await;

        assert!(res.is_err());
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
