// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

#[cfg(feature = "pg_integration")]
mod tests {
    use rand::rngs::StdRng;
    use rand::SeedableRng;
    use serial_test::serial;
    use simulacrum::Simulacrum;
    use std::path::PathBuf;
    use std::sync::Arc;
    use sui_graphql_rpc::config::ConnectionConfig;
    use sui_graphql_rpc::examples::{load_examples, ExampleQuery, ExampleQueryGroup};
    use sui_graphql_rpc::test_infra::cluster::ExecutorCluster;
    use sui_graphql_rpc::test_infra::cluster::DEFAULT_INTERNAL_DATA_SOURCE_PORT;

    fn bad_examples() -> ExampleQueryGroup {
        ExampleQueryGroup {
            name: "bad_examples".to_string(),
            queries: vec![
                ExampleQuery {
                    name: "multiple_queries".to_string(),
                    contents: "{ chainIdentifier } { chainIdentifier }".to_string(),
                    path: PathBuf::from("multiple_queries.graphql"),
                },
                ExampleQuery {
                    name: "malformed".to_string(),
                    contents: "query { }}".to_string(),
                    path: PathBuf::from("malformed.graphql"),
                },
                ExampleQuery {
                    name: "invalid".to_string(),
                    contents: "djewfbfo".to_string(),
                    path: PathBuf::from("invalid.graphql"),
                },
                ExampleQuery {
                    name: "empty".to_string(),
                    contents: "     ".to_string(),
                    path: PathBuf::from("empty.graphql"),
                },
            ],
            _path: PathBuf::from("bad_examples"),
        }
    }

    async fn validate_example_query_group(
        cluster: &ExecutorCluster,
        group: &ExampleQueryGroup,
    ) -> Vec<String> {
        let mut errors = vec![];
        for query in &group.queries {
            let resp = cluster
                .graphql_client
                .execute(query.contents.clone(), vec![])
                .await
                .unwrap();
            if let Some(err) = resp.get("errors") {
                errors.push(format!(
                    "Query failed: {}: {} at: {}\nError: {}",
                    group.name,
                    query.name,
                    query.path.display(),
                    err
                ));
            }
        }
        errors
    }

    #[tokio::test]
    #[serial]
    async fn test_single_all_examples_structure_valid() {
        let rng = StdRng::from_seed([12; 32]);
        let mut sim = Simulacrum::new_with_rng(rng);

        sim.create_checkpoint();

        let connection_config = ConnectionConfig::ci_integration_test_cfg();

        let cluster = sui_graphql_rpc::test_infra::cluster::serve_executor(
            connection_config,
            DEFAULT_INTERNAL_DATA_SOURCE_PORT,
            Arc::new(sim),
        )
        .await;

        let groups = load_examples().expect("Could not load examples");

        let mut errors = vec![];
        for group in groups {
            errors.extend(validate_example_query_group(&cluster, &group).await);
        }

        assert!(errors.is_empty(), "\n{}", errors.join("\n\n"));
    }

    #[tokio::test]
    #[serial]
    async fn test_bad_examples_fail() {
        let rng = StdRng::from_seed([12; 32]);
        let mut sim = Simulacrum::new_with_rng(rng);

        sim.create_checkpoint();

        let connection_config = ConnectionConfig::ci_integration_test_cfg();

        let cluster = sui_graphql_rpc::test_infra::cluster::serve_executor(
            connection_config,
            DEFAULT_INTERNAL_DATA_SOURCE_PORT,
            Arc::new(sim),
        )
        .await;

        let bad_examples = bad_examples();
        let errors = validate_example_query_group(&cluster, &bad_examples).await;

        assert_eq!(
            errors.len(),
            bad_examples.queries.len(),
            "all examples should fail"
        );
    }
}
