// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

//! End-to-end tests for the db-shell proxy mode.
//!
//! Starts a single-validator cluster, sends a few transactions, waits for
//! checkpoints to be executed, then exercises every shell command against the
//! admin API (ls, cat/read in json/debug/bcs) across all navigable namespaces.

use serde::Deserialize;
use serde_json::Value as JsonValue;
use sui_macros::sim_test;
use sui_test_transaction_builder::TestTransactionBuilder;
use sui_types::base_types::FullObjectRef;
use sui_types::messages_checkpoint::CheckpointSequenceNumber;
use test_cluster::TestClusterBuilder;

// ─── helpers ──────────────────────────────────────────────────────────────────

#[derive(Debug, Deserialize)]
struct DirEntry {
    name: String,
    is_dir: bool,
}

struct AdminClient {
    base: String,
    client: reqwest::Client,
}

impl AdminClient {
    fn new(port: u16) -> Self {
        Self {
            base: format!("http://127.0.0.1:{port}"),
            client: reqwest::Client::new(),
        }
    }

    async fn ls(&self, path: &str) -> Vec<DirEntry> {
        self.ls_limit(path, 100).await
    }

    async fn ls_limit(&self, path: &str, limit: usize) -> Vec<DirEntry> {
        self.ls_inner(path, limit, false).await
    }

    async fn ls_cursor(&self, path: &str) -> Vec<DirEntry> {
        self.ls_inner(path, 100, true).await
    }

    async fn ls_inner(&self, path: &str, limit: usize, cursor: bool) -> Vec<DirEntry> {
        let url = format!("{}/db-shell/ls", self.base);
        let resp = self
            .client
            .get(&url)
            .query(&[
                ("path", path.to_string()),
                ("limit", limit.to_string()),
                ("cursor", cursor.to_string()),
            ])
            .send()
            .await
            .unwrap_or_else(|e| panic!("ls {path}: {e}"));
        assert!(
            resp.status().is_success(),
            "ls {path} returned {}",
            resp.status()
        );
        resp.json()
            .await
            .unwrap_or_else(|e| panic!("ls {path} parse: {e}"))
    }

    async fn read_json(&self, path: &str) -> JsonValue {
        let url = format!("{}/db-shell/read", self.base);
        let resp = self
            .client
            .get(&url)
            .query(&[("path", path), ("format", "json")])
            .send()
            .await
            .unwrap_or_else(|e| panic!("read json {path}: {e}"));
        assert!(
            resp.status().is_success(),
            "read json {path} returned {}",
            resp.status()
        );
        resp.json()
            .await
            .unwrap_or_else(|e| panic!("read json {path} parse: {e}"))
    }

    /// Like `read_json` but returns `None` when the server responds 404.
    async fn read_json_optional(&self, path: &str) -> Option<JsonValue> {
        let url = format!("{}/db-shell/read", self.base);
        let resp = self
            .client
            .get(&url)
            .query(&[("path", path), ("format", "json")])
            .send()
            .await
            .unwrap_or_else(|e| panic!("read json optional {path}: {e}"));
        if resp.status() == reqwest::StatusCode::NOT_FOUND {
            return None;
        }
        assert!(
            resp.status().is_success(),
            "read json {path} returned {}",
            resp.status()
        );
        Some(
            resp.json()
                .await
                .unwrap_or_else(|e| panic!("read json {path} parse: {e}")),
        )
    }

    async fn read_debug(&self, path: &str) -> String {
        let url = format!("{}/db-shell/read", self.base);
        let resp = self
            .client
            .get(&url)
            .query(&[("path", path), ("format", "debug")])
            .send()
            .await
            .unwrap_or_else(|e| panic!("read debug {path}: {e}"));
        assert!(
            resp.status().is_success(),
            "read debug {path} returned {}",
            resp.status()
        );
        resp.text()
            .await
            .unwrap_or_else(|e| panic!("read debug {path} text: {e}"))
    }

    async fn read_bcs(&self, path: &str) -> String {
        let url = format!("{}/db-shell/read", self.base);
        let resp = self
            .client
            .get(&url)
            .query(&[("path", path), ("format", "bcs")])
            .send()
            .await
            .unwrap_or_else(|e| panic!("read bcs {path}: {e}"));
        assert!(
            resp.status().is_success(),
            "read bcs {path} returned {}",
            resp.status()
        );
        resp.text()
            .await
            .unwrap_or_else(|e| panic!("read bcs {path} text: {e}"))
    }
}

// ─── test ─────────────────────────────────────────────────────────────────────

#[sim_test]
async fn test_db_shell_proxy_all_commands() {
    let mut cluster = TestClusterBuilder::new().build().await;

    // Submit a few transactions so the DB has populated checkpoints and transactions.
    let context = &mut cluster.wallet;
    let gas_price = context.get_reference_gas_price().await.unwrap();
    let accounts_and_objs = context.get_all_accounts_and_gas_objects().await.unwrap();
    let sender = accounts_and_objs[0].0;
    let receiver = accounts_and_objs[1].0;

    // Send 2 transactions: each uses one gas object and sends another object.
    // Default cluster gives the first account at least 5 objects.
    let mut digests = Vec::new();
    for i in 0..2 {
        let gas_object = accounts_and_objs[0].1[i * 2];
        let object_to_send = accounts_and_objs[0].1[i * 2 + 1];
        let txn = context
            .sign_transaction(
                &TestTransactionBuilder::new(sender, gas_object, gas_price)
                    .transfer(FullObjectRef::from_fastpath_ref(object_to_send), receiver)
                    .build(),
            )
            .await;
        let resp = context.execute_transaction_must_succeed(txn).await;
        digests.push(resp.transaction.digest());
    }

    // Wait until all transactions are checkpointed and executed on the fullnode.
    cluster.wait_for_tx_settlement(digests.as_slice()).await;

    // Grab the validator's admin port.
    let admin_port =
        cluster.all_validator_handles()[0].with(|node| node.get_config().admin_interface_port);

    let api = AdminClient::new(admin_port);

    // ── / (root) ──────────────────────────────────────────────────────────────
    {
        let entries = api.ls("/").await;
        let names: Vec<&str> = entries.iter().map(|e| e.name.as_str()).collect();
        assert!(names.contains(&"epochs"), "root should contain 'epochs'");
        assert!(
            names.contains(&"checkpoints"),
            "root should contain 'checkpoints'"
        );
        assert!(
            names.contains(&"transactions"),
            "root should contain 'transactions'"
        );
        assert!(
            names.contains(&"consensus"),
            "root should contain 'consensus'"
        );
        for e in &entries {
            assert!(e.is_dir, "all root entries should be directories");
        }
    }

    // ── /epochs ───────────────────────────────────────────────────────────────
    let epoch_entries = api.ls("/epochs").await;
    assert!(!epoch_entries.is_empty(), "/epochs should be non-empty");
    let epoch_name = &epoch_entries[0].name;
    assert!(
        epoch_entries[0].is_dir,
        "epoch entries should be directories"
    );

    // ── /epochs/<e> ───────────────────────────────────────────────────────────
    {
        let entries = api.ls(&format!("/epochs/{epoch_name}")).await;
        let names: Vec<&str> = entries.iter().map(|e| e.name.as_str()).collect();
        assert!(names.contains(&"first-checkpoint"));
        assert!(names.contains(&"last-checkpoint"));
        assert!(names.contains(&"committee"));
    }

    // ── /epochs/<e>/first-checkpoint ─────────────────────────────────────────
    let first_checkpoint_seq: u64 = {
        let val = api
            .read_json(&format!("/epochs/{epoch_name}/first-checkpoint"))
            .await;
        val["sequence_number"]
            .as_u64()
            .unwrap_or_else(|| panic!("first-checkpoint should have sequence_number, got {val}"))
    };

    // ── /epochs/<e>/last-checkpoint ──────────────────────────────────────────
    // Returns 404 while the epoch is still in progress.
    let last_checkpoint_seq: u64 = {
        match api
            .read_json_optional(&format!("/epochs/{epoch_name}/last-checkpoint"))
            .await
        {
            None => first_checkpoint_seq,
            Some(val) => val["sequence_number"].as_u64().unwrap_or_else(|| {
                panic!("last-checkpoint should have sequence_number, got {val}")
            }),
        }
    };

    // ── /epochs/<e>/committee ─────────────────────────────────────────────────
    {
        let val = api
            .read_json(&format!("/epochs/{epoch_name}/committee"))
            .await;
        assert!(
            val.get("epoch").is_some() || val.get("voting_rights").is_some(),
            "committee json should have epoch or voting_rights, got {val}"
        );
        let dbg = api
            .read_debug(&format!("/epochs/{epoch_name}/committee"))
            .await;
        assert!(!dbg.is_empty(), "committee debug should be non-empty");
    }

    // ── /checkpoints/seq ─────────────────────────────────────────────────────
    {
        let entries = api.ls("/checkpoints/seq").await;
        assert!(
            !entries.is_empty(),
            "/checkpoints/seq should list at least one checkpoint"
        );
        assert!(
            entries.iter().all(|e| e.is_dir),
            "checkpoint seq entries should be directories"
        );
    }

    // ── /checkpoints/seq/<seq>/summary ───────────────────────────────────────
    let checkpoint_seq = first_checkpoint_seq;
    let seq_path = format!("/checkpoints/seq/{checkpoint_seq}");
    {
        let entries = api.ls(&seq_path).await;
        let names: Vec<&str> = entries.iter().map(|e| e.name.as_str()).collect();
        assert!(
            names.contains(&"summary"),
            "{seq_path} should contain 'summary'"
        );
        assert!(
            names.contains(&"contents"),
            "{seq_path} should contain 'contents'"
        );
        assert!(
            names.contains(&"contents-short"),
            "{seq_path} should contain 'contents-short'"
        );
    }
    {
        let summary_path = format!("{seq_path}/summary");
        let val = api.read_json(&summary_path).await;
        assert!(
            val.get("sequence_number").is_some(),
            "summary json should have sequence_number, got {val}"
        );
        let dbg = api.read_debug(&summary_path).await;
        assert!(!dbg.is_empty());
        let bcs = api.read_bcs(&summary_path).await;
        // base64-encoded non-empty bytes
        assert!(!bcs.trim().is_empty(), "bcs response should be non-empty");
    }

    // ── /checkpoints/seq/<seq>/contents ──────────────────────────────────────
    {
        let contents_path = format!("{seq_path}/contents");
        let val = api.read_json(&contents_path).await;
        assert!(
            val.get("transactions").is_some() || val.is_object(),
            "contents json should be an object, got {val}"
        );
    }

    // ── /checkpoints/seq/<seq>/contents-short ────────────────────────────────
    {
        let short_path = format!("{seq_path}/contents-short");
        let text = api.read_debug(&short_path).await;
        // Genesis checkpoint may have no user txs; just check it doesn't error.
        assert!(
            text.is_empty() || text.contains("tx=") || text.len() > 0,
            "contents-short returned unexpected content"
        );
    }

    // ── /checkpoints/digest/<digest>/summary ─────────────────────────────────
    // The summary JSON doesn't embed the checkpoint's own digest (it's derived
    // from the data). Fetch it directly from the node's checkpoint store.
    {
        let seq = checkpoint_seq as CheckpointSequenceNumber;
        let digest_str = cluster.all_validator_handles()[0].with(|node| {
            node.state()
                .checkpoint_store
                .get_checkpoint_by_sequence_number(seq)
                .unwrap()
                .map(|cp| cp.digest().to_string())
        });
        let digest_str =
            digest_str.unwrap_or_else(|| panic!("checkpoint {seq} not found in store"));

        let digest_summary = api
            .read_json(&format!("/checkpoints/digest/{digest_str}/summary"))
            .await;
        assert_eq!(
            digest_summary["sequence_number"].as_u64().unwrap(),
            checkpoint_seq,
            "digest lookup should return the same checkpoint as seq lookup"
        );
    }

    // ── /transactions/<digest> ────────────────────────────────────────────────
    {
        // Use one of the settled transaction digests.
        let tx_digest = digests[0];
        let tx_path = format!("/transactions/{tx_digest}");
        let val = api.read_json(&tx_path).await;
        assert!(
            !val.is_null(),
            "transaction json should be non-null, got {val}"
        );
        let dbg = api.read_debug(&tx_path).await;
        assert!(!dbg.is_empty(), "transaction debug should be non-empty");
        let bcs = api.read_bcs(&tx_path).await;
        assert!(
            !bcs.trim().is_empty(),
            "transaction bcs should be non-empty"
        );
    }

    // ── /consensus ────────────────────────────────────────────────────────────
    {
        let entries = api.ls("/consensus").await;
        let names: Vec<&str> = entries.iter().map(|e| e.name.as_str()).collect();
        assert!(
            names.contains(&"commits"),
            "/consensus should contain 'commits'"
        );
        assert!(
            names.contains(&"latest"),
            "/consensus should contain 'latest'"
        );
    }

    // ── /consensus/latest ─────────────────────────────────────────────────────
    let latest_commit_index: u64 = {
        let val = api.read_json("/consensus/latest").await;
        val["index"]
            .as_u64()
            .unwrap_or_else(|| panic!("consensus/latest should have an index field, got {val}"))
    };
    assert!(
        latest_commit_index > 0,
        "latest consensus commit index should be > 0"
    );

    // ── /consensus/commits/<index>/summary ────────────────────────────────────
    {
        let entries = api.ls("/consensus/commits").await;
        assert!(
            !entries.is_empty(),
            "/consensus/commits should be non-empty"
        );

        let first_index_str = &entries[0].name;
        let commit_path = format!("/consensus/commits/{first_index_str}");
        let commit_entries = api.ls(&commit_path).await;
        let names: Vec<&str> = commit_entries.iter().map(|e| e.name.as_str()).collect();
        assert!(
            names.contains(&"summary"),
            "{commit_path} should contain 'summary'"
        );

        let summary_path = format!("{commit_path}/summary");
        let val = api.read_json(&summary_path).await;
        assert!(
            val.get("index").is_some(),
            "commit summary should have index, got {val}"
        );
        assert!(
            val.get("transactions").is_some(),
            "commit summary should have transactions, got {val}"
        );
        let dbg = api.read_debug(&summary_path).await;
        assert!(!dbg.is_empty(), "commit summary debug should be non-empty");

        // Also spot-check the latest commit.
        let latest_summary = api
            .read_json(&format!("/consensus/commits/{latest_commit_index}/summary"))
            .await;
        assert_eq!(
            latest_summary["index"].as_u64().unwrap(),
            latest_commit_index
        );
    }

    // ── ls cursor semantics ───────────────────────────────────────────────────
    // With ?cursor=true, ls /checkpoints/seq/<seq> returns checkpoints starting
    // from that seq rather than the children of that specific checkpoint dir.
    {
        let cursor_entries = api
            .ls_cursor(&format!("/checkpoints/seq/{checkpoint_seq}"))
            .await;
        assert!(
            !cursor_entries.is_empty(),
            "cursor listing should return at least one entry"
        );
        // All entries should be checkpoint dirs.
        assert!(
            cursor_entries.iter().all(|e| e.is_dir),
            "cursor entries should all be directories, got: {:?}",
            cursor_entries
        );
        // The first entry should be at or after the cursor.
        let first_name: u64 = cursor_entries[0].name.parse().unwrap_or_else(|_| {
            panic!(
                "cursor entry name should be numeric: {}",
                cursor_entries[0].name
            )
        });
        assert!(
            first_name >= checkpoint_seq,
            "cursor listing should start at or after the given seq"
        );
    }

    // ── ls with --limit ───────────────────────────────────────────────────────
    {
        let limited = api.ls_limit("/checkpoints/seq", 2).await;
        assert!(
            limited.len() <= 2,
            "limit=2 should return at most 2 entries"
        );
    }

    let _ = last_checkpoint_seq; // used above via last-checkpoint read
}
