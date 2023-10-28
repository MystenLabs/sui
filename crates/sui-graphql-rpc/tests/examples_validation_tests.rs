// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use std::fs::File;
use std::io::Read;
use std::path::PathBuf;

#[cfg(feature = "pg_integration")]
mod tests {
    use rand::rngs::StdRng;
    use rand::SeedableRng;
    use serial_test::serial;
    use simulacrum::Simulacrum;
    use std::path::PathBuf;
    use std::sync::Arc;
    use sui_graphql_rpc::cluster::SimulatorCluster;
    use sui_graphql_rpc::config::ConnectionConfig;
    use sui_graphql_rpc::examples::{load_examples, ExampleQuery, ExampleQueryGroup};

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
        cluster: &SimulatorCluster,
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

        let cluster =
            sui_graphql_rpc::cluster::serve_simulator(connection_config, 3000, Arc::new(sim)).await;

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

        let cluster =
            sui_graphql_rpc::cluster::serve_simulator(connection_config, 3000, Arc::new(sim)).await;

        let bad_examples = bad_examples();
        let errors = validate_example_query_group(&cluster, &bad_examples).await;

        assert_eq!(
            errors.len(),
            bad_examples.queries.len(),
            "all examples should fail"
        );
    }
}

#[test]
fn test_generate_markdown() {
    let mut buf: PathBuf = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    buf.push("docs");
    buf.push("examples.md");
    let mut out_file: File = File::open(buf).expect("Could not open examples.md");

    println!("Writing examples to: {:?}", out_file);
    // Read the current content of `out_file`
    let mut current_content = String::new();
    out_file
        .read_to_string(&mut current_content)
        .expect("Could not read examples.md");
    let new_content: String = sui_graphql_rpc::examples::generate_markdown()
        .expect("Generating examples markdown failed");
    assert_eq!(current_content, new_content, "Doc examples have changed. Please run `sui-graphql-rpc generate-examples` to update the docs.");
}
