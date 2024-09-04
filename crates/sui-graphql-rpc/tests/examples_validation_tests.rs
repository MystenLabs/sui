// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use anyhow::{anyhow, Context, Result};
use std::cmp::max;
use std::collections::BTreeMap;
use std::fs;
use std::path::PathBuf;
use sui_graphql_rpc::config::Limits;
use sui_graphql_rpc::test_infra::cluster::{prep_executor_cluster, ExecutorCluster};

struct Example {
    contents: String,
    path: Option<PathBuf>,
}

fn good_examples() -> Result<BTreeMap<String, Example>> {
    let examples = PathBuf::from(&env!("CARGO_MANIFEST_DIR")).join("examples");

    let mut dirs = vec![examples.clone()];
    let mut queries = BTreeMap::new();
    while let Some(dir) = dirs.pop() {
        let entries =
            fs::read_dir(&dir).with_context(|| format!("Looking in {}", dir.display()))?;

        for entry in entries {
            let entry = entry.with_context(|| format!("Entry in {}", dir.display()))?;
            let path = entry.path();
            let typ_ = entry
                .file_type()
                .with_context(|| format!("Metadata for {}", path.display()))?;

            if typ_.is_dir() {
                dirs.push(entry.path());
                continue;
            }

            if path.ends_with(".graphql") {
                let contents = fs::read_to_string(&path)
                    .with_context(|| format!("Reading {}", path.display()))?;

                let rel_path = path
                    .strip_prefix(&examples)
                    .with_context(|| format!("Generating name from {}", path.display()))?
                    .with_extension("");

                let name = rel_path
                    .to_str()
                    .ok_or_else(|| anyhow!("Generating name from {}", path.display()))?;

                queries.insert(
                    name.to_string(),
                    Example {
                        contents,
                        path: Some(path),
                    },
                );
            }
        }
    }

    Ok(queries)
}

fn bad_examples() -> BTreeMap<String, Example> {
    BTreeMap::from_iter([
        (
            "multiple_queries".to_string(),
            Example {
                contents: "{ chainIdentifier } { chainIdentifier }".to_string(),
                path: None,
            },
        ),
        (
            "malformed".to_string(),
            Example {
                contents: "query { }}".to_string(),
                path: None,
            },
        ),
        (
            "invalid".to_string(),
            Example {
                contents: "djewfbfo".to_string(),
                path: None,
            },
        ),
        (
            "empty".to_string(),
            Example {
                contents: "     ".to_string(),
                path: None,
            },
        ),
    ])
}

async fn test_query(
    cluster: &ExecutorCluster,
    name: &str,
    query: &Example,
    max_nodes: &mut u64,
    max_output_nodes: &mut u64,
    max_depth: &mut u64,
    max_payload: &mut u64,
) -> Vec<String> {
    let resp = cluster
        .graphql_client
        .execute_to_graphql(query.contents.clone(), true, vec![], vec![])
        .await
        .unwrap();

    let errors = resp.errors();
    if errors.is_empty() {
        let usage = resp
            .usage()
            .expect("Usage not found")
            .expect("Usage not found");
        *max_nodes = max(*max_nodes, usage["inputNodes"]);
        *max_output_nodes = max(*max_output_nodes, usage["outputNodes"]);
        *max_depth = max(*max_depth, usage["depth"]);
        *max_payload = max(*max_payload, usage["queryPayload"]);
        return vec![];
    }

    errors
        .into_iter()
        .map(|e| match &query.path {
            Some(p) => format!("Query {name:?} at {} failed: {e}", p.display()),
            None => format!("Query {name:?} failed: {e}"),
        })
        .collect()
}

#[tokio::test]
async fn good_examples_within_limits() {
    let cluster = prep_executor_cluster().await;
    let (mut max_nodes, mut max_output_nodes, mut max_depth, mut max_payload) = (0, 0, 0, 0);

    let mut errors = vec![];
    for (name, example) in good_examples().expect("Could not load examples") {
        errors.extend(
            test_query(
                &cluster,
                &name,
                &example,
                &mut max_nodes,
                &mut max_output_nodes,
                &mut max_depth,
                &mut max_payload,
            )
            .await,
        );
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
        max_output_nodes <= default_config.max_output_nodes as u64,
        "Max output nodes {} exceeds default limit {}",
        max_output_nodes,
        default_config.max_output_nodes
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
async fn bad_examples_fail() {
    let cluster = prep_executor_cluster().await;
    let (mut max_nodes, mut max_output_nodes, mut max_depth, mut max_payload) = (0, 0, 0, 0);

    for (name, example) in bad_examples() {
        let errors = test_query(
            &cluster,
            &name,
            &example,
            &mut max_nodes,
            &mut max_output_nodes,
            &mut max_depth,
            &mut max_payload,
        )
        .await;

        assert!(!errors.is_empty(), "Query {name:?} should have failed");
    }
}
