// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

#[cfg(feature = "pg_integration")]
mod tests {
    use rand::rngs::StdRng;
    use rand::SeedableRng;
    use serial_test::serial;
    use simulacrum::Simulacrum;
    use std::cmp::max;
    use std::path::PathBuf;
    use std::sync::Arc;
    use sui_graphql_rpc::config::{ConnectionConfig, Limits};
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
        max_nodes: &mut u64,
        max_depth: &mut u64,
        max_payload: &mut u64,
    ) -> Vec<String> {
        let mut errors = vec![];
        for query in &group.queries {
            let resp = cluster
                .graphql_client
                .execute_to_graphql(query.contents.clone(), true, vec![], vec![])
                .await
                .unwrap();
            resp.errors().iter().for_each(|err| {
                errors.push(format!(
                    "Query failed: {}: {} at: {}\nError: {}",
                    group.name,
                    query.name,
                    query.path.display(),
                    err
                ))
            });
            if resp.errors().is_empty() {
                let usage = resp
                    .usage()
                    .expect("Usage fetch should succeed")
                    .unwrap_or_else(|| panic!("Usage should be present for query: {}", query.name));

                let nodes = *usage.get("nodes").unwrap_or_else(|| {
                    panic!("Node usage should be present for query: {}", query.name)
                });
                let depth = *usage.get("depth").unwrap_or_else(|| {
                    panic!("Depth usage should be present for query: {}", query.name)
                });
                let payload = *usage.get("query_payload").unwrap_or_else(|| {
                    panic!("Payload usage should be present for query: {}", query.name)
                });
                *max_nodes = max(*max_nodes, nodes);
                *max_depth = max(*max_depth, depth);
                *max_payload = max(*max_payload, payload);
            }
        }
        errors
    }

    #[tokio::test]
    #[serial]
    async fn test_single_all_examples_structure_valid() {
        let rng = StdRng::from_seed([12; 32]);
        let mut sim = Simulacrum::new_with_rng(rng);
        let (mut max_nodes, mut max_depth, mut max_payload) = (0, 0, 0);

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
            let group_errors = validate_example_query_group(
                &cluster,
                &group,
                &mut max_nodes,
                &mut max_depth,
                &mut max_payload,
            )
            .await;
            errors.extend(group_errors);
        }

        // Check that our examples can run with our usage limits
        let default_config = Limits::default();
        assert!(
            max_nodes <= default_config.max_query_nodes as u64,
            "Max nodes {} exceeds default limit {}",
            max_nodes,
            default_config.max_query_nodes
        );
        assert!(
            max_depth <= default_config.max_query_depth as u64,
            "Max depth {} exceeds default limit {}",
            max_depth,
            default_config.max_query_depth
        );
        assert!(
            max_payload <= default_config.max_query_payload_size as u64,
            "Max payload {} exceeds default limit {}",
            max_payload,
            default_config.max_query_payload_size
        );

        assert!(errors.is_empty(), "\n{}", errors.join("\n\n"));
    }

    #[tokio::test]
    #[serial]
    async fn test_bad_examples_fail() {
        let rng = StdRng::from_seed([12; 32]);
        let mut sim = Simulacrum::new_with_rng(rng);
        let (mut max_nodes, mut max_depth, mut max_payload) = (0, 0, 0);

        sim.create_checkpoint();

        let connection_config = ConnectionConfig::ci_integration_test_cfg();

        let cluster = sui_graphql_rpc::test_infra::cluster::serve_executor(
            connection_config,
            DEFAULT_INTERNAL_DATA_SOURCE_PORT,
            Arc::new(sim),
        )
        .await;

        let bad_examples = bad_examples();
        let errors = validate_example_query_group(
            &cluster,
            &bad_examples,
            &mut max_nodes,
            &mut max_depth,
            &mut max_payload,
        )
        .await;

        assert_eq!(
            errors.len(),
            bad_examples.queries.len(),
            "all examples should fail"
        );
    }
}
