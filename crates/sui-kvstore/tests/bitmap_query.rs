// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

//! Integration tests for the bitmap index scanning and streaming pipeline.
//!
//! Spawns a BigTable emulator and writes synthetic data directly, then verifies:
//! - `eval_bitmap_query_stream` composes DNF terms and signed literals correctly
//! - `checkpoint_to_tx_range` resolves bounds
//! - `resolve_tx_digests` resolves sequence numbers to digests via tx_seq_digest

use std::ops::Range;

use anyhow::Context;
use anyhow::Result;
use bytes::Bytes;
use futures::TryStreamExt;
use roaring::RoaringBitmap;
use sui_inverted_index::IndexDimension;
use sui_inverted_index::encode_dimension_key;
use sui_inverted_index::eval_bitmap_query_stream;
use sui_kvstore::BigTableBitmapSource;
use sui_kvstore::BigTableClient;
use sui_kvstore::BitmapIndexSpec;
use sui_kvstore::BitmapLiteral;
use sui_kvstore::BitmapQuery;
use sui_kvstore::BitmapTerm;
use sui_kvstore::ScanDirection;
use sui_kvstore::TxSeqDigestData;
use sui_kvstore::tables;
use sui_kvstore::tables::transaction_bitmap_index;
use sui_kvstore::testing::BigTableEmulator;
use sui_kvstore::testing::INSTANCE_ID;
use sui_kvstore::testing::create_tables;
use sui_kvstore::testing::require_bigtable_emulator;
use sui_types::digests::CheckpointContentsDigest;
use sui_types::digests::TransactionDigest;
use sui_types::messages_checkpoint::CheckpointSequenceNumber;
use sui_types::messages_checkpoint::CheckpointSummary;

/// Create a BigTable client connected to a fresh emulator with all tables created.
async fn setup_emulator() -> Result<(BigTableClient, BigTableEmulator)> {
    require_bigtable_emulator();
    let emulator = tokio::task::spawn_blocking(BigTableEmulator::start)
        .await
        .context("spawn_blocking panicked")??;
    create_tables(emulator.host(), INSTANCE_ID).await?;
    let client = BigTableClient::new_local(emulator.host().to_string(), INSTANCE_ID.to_string())
        .await
        .context("Failed to create BigTable client")?;
    Ok((client, emulator))
}

fn bitmap_query_stream(
    client: &BigTableClient,
    query: BitmapQuery,
    range: Range<u64>,
    spec: BitmapIndexSpec,
    direction: ScanDirection,
) -> impl futures::Stream<Item = Result<u64>> + Send + 'static {
    let source = BigTableBitmapSource::new(client.clone(), spec);
    eval_bitmap_query_stream(source, query, range, spec.bucket_size, direction)
}

/// Write a bitmap index entry directly. Serializes a RoaringBitmap containing
/// the given bit positions into the bitmap column of the given row key.
async fn write_bitmap(
    client: &mut BigTableClient,
    dimension_key: &[u8],
    bucket_id: u64,
    bits: &[u32],
) -> Result<()> {
    let mut bm = RoaringBitmap::new();
    for &bit in bits {
        bm.insert(bit);
    }
    let mut buf = Vec::new();
    bm.serialize_into(&mut buf)?;

    let row_key = transaction_bitmap_index::encode_row_key(
        transaction_bitmap_index::SCHEMA_VERSION,
        dimension_key,
        bucket_id,
    );
    let entry = tables::make_entry(
        row_key,
        vec![(transaction_bitmap_index::col::BITMAP, Bytes::from(buf))],
        None,
    );
    client
        .write_entries(transaction_bitmap_index::NAME, vec![entry])
        .await
}

/// Write a tx_seq_digest mapping entry: tx_seq → (digest, event_count=0).
async fn write_tx_seq_digest(
    client: &mut BigTableClient,
    tx_seq: u64,
    digest: &TransactionDigest,
    checkpoint_number: CheckpointSequenceNumber,
) -> Result<()> {
    let entry = tables::make_entry(
        tables::tx_seq_digest::encode_key(tx_seq),
        tables::tx_seq_digest::encode(digest, 0, checkpoint_number),
        None,
    );
    client
        .write_entries(tables::tx_seq_digest::NAME, vec![entry])
        .await
}

fn tx_seq_digest_data(
    tx_sequence_number: u64,
    digest: TransactionDigest,
    checkpoint_number: CheckpointSequenceNumber,
) -> TxSeqDigestData {
    TxSeqDigestData {
        tx_sequence_number,
        digest,
        event_count: 0,
        checkpoint_number,
    }
}

/// Convenience: write tx_seq_digest rows for a run of consecutive tx_seqs.
async fn write_tx_seq_digests(
    client: &mut BigTableClient,
    tx_lo: u64,
    digests: &[TransactionDigest],
    checkpoint_number: CheckpointSequenceNumber,
) -> Result<()> {
    for (i, d) in digests.iter().enumerate() {
        write_tx_seq_digest(client, tx_lo + i as u64, d, checkpoint_number).await?;
    }
    Ok(())
}

/// Write a minimal checkpoint summary (only the fields needed for checkpoint_to_tx_range).
async fn write_checkpoint_summary(
    client: &mut BigTableClient,
    seq: CheckpointSequenceNumber,
    network_total_transactions: u64,
) -> Result<()> {
    // Build a minimal summary. Most fields are zeroed out.
    let summary = CheckpointSummary {
        epoch: 0,
        sequence_number: seq,
        network_total_transactions,
        content_digest: CheckpointContentsDigest::new([0; 32]),
        previous_digest: None,
        epoch_rolling_gas_cost_summary: Default::default(),
        timestamp_ms: 1000 * (seq + 1),
        checkpoint_commitments: vec![],
        end_of_epoch_data: None,
        version_specific_data: vec![],
    };

    let entry = tables::make_entry(
        tables::checkpoints::encode_key(seq),
        vec![(
            tables::checkpoints::col::SUMMARY,
            Bytes::from(bcs::to_bytes(&summary)?),
        )],
        Some(summary.timestamp_ms),
    );
    client
        .write_entries(tables::checkpoints::NAME, vec![entry])
        .await
}

/// Write a minimal transaction entry (just enough for get_transactions to return).
async fn write_transaction(
    client: &mut BigTableClient,
    digest: &TransactionDigest,
    checkpoint_number: u64,
    timestamp_ms: u64,
) -> Result<()> {
    use sui_kvstore::tables::transactions;

    let entry = tables::make_entry(
        transactions::encode_key(digest),
        vec![
            (
                transactions::col::CHECKPOINT_NUMBER,
                Bytes::from(bcs::to_bytes(&checkpoint_number)?),
            ),
            (
                transactions::col::TIMESTAMP,
                Bytes::from(bcs::to_bytes(&timestamp_ms)?),
            ),
        ],
        Some(timestamp_ms),
    );
    client.write_entries(transactions::NAME, vec![entry]).await
}

// ---- Tests ----

#[tokio::test]
async fn test_eval_bitmap_query_and() -> Result<()> {
    let (mut client, _emulator) = setup_emulator().await?;

    let dim_a = encode_dimension_key(IndexDimension::Sender, &[0xAA; 32]);
    let dim_b = encode_dimension_key(IndexDimension::MoveCall, &[0xBB; 32]);

    // dim_a has tx_seqs: 1, 2, 3, 4, 5
    write_bitmap(&mut client, &dim_a, 0, &[1, 2, 3, 4, 5]).await?;
    // dim_b has tx_seqs: 3, 4, 5, 6, 7
    write_bitmap(&mut client, &dim_b, 0, &[3, 4, 5, 6, 7]).await?;

    let query = BitmapQuery::new(vec![BitmapTerm::new(vec![
        BitmapLiteral::include(dim_a)?,
        BitmapLiteral::include(dim_b)?,
    ])?])?;
    let result: Vec<u64> = bitmap_query_stream(
        &client,
        query,
        0..100_000,
        BitmapIndexSpec::tx(),
        ScanDirection::Ascending,
    )
    .try_collect()
    .await?;
    assert_eq!(result, vec![3, 4, 5]);

    Ok(())
}

#[tokio::test]
async fn test_eval_bitmap_query_or() -> Result<()> {
    let (mut client, _emulator) = setup_emulator().await?;

    let dim_a = encode_dimension_key(IndexDimension::Sender, &[0xCC; 32]);
    let dim_b = encode_dimension_key(IndexDimension::AffectedAddress, &[0xDD; 32]);

    write_bitmap(&mut client, &dim_a, 0, &[1, 2, 3]).await?;
    write_bitmap(&mut client, &dim_b, 0, &[3, 4, 5]).await?;

    let query = BitmapQuery::new(vec![
        BitmapTerm::new(vec![BitmapLiteral::include(dim_a)?])?,
        BitmapTerm::new(vec![BitmapLiteral::include(dim_b)?])?,
    ])?;
    let result: Vec<u64> = bitmap_query_stream(
        &client,
        query,
        0..100_000,
        BitmapIndexSpec::tx(),
        ScanDirection::Ascending,
    )
    .try_collect()
    .await?;
    assert_eq!(result, vec![1, 2, 3, 4, 5]);

    Ok(())
}

#[tokio::test]
async fn test_eval_bitmap_query_not() -> Result<()> {
    let (mut client, _emulator) = setup_emulator().await?;

    let dim_a = encode_dimension_key(IndexDimension::Sender, &[0xEE; 32]);
    let dim_b = encode_dimension_key(IndexDimension::MoveCall, &[0xFF; 32]);

    // dim_a: tx_seqs 1..=5
    write_bitmap(&mut client, &dim_a, 0, &[1, 2, 3, 4, 5]).await?;
    // dim_b: tx_seqs 3, 4
    write_bitmap(&mut client, &dim_b, 0, &[3, 4]).await?;

    // "sender EE but NOT move_call FF" → [1, 2, 5]
    let query = BitmapQuery::new(vec![BitmapTerm::new(vec![
        BitmapLiteral::include(dim_a)?,
        BitmapLiteral::exclude(dim_b)?,
    ])?])?;
    let result: Vec<u64> = bitmap_query_stream(
        &client,
        query,
        0..100_000,
        BitmapIndexSpec::tx(),
        ScanDirection::Ascending,
    )
    .try_collect()
    .await?;
    assert_eq!(result, vec![1, 2, 5]);

    Ok(())
}

#[tokio::test]
async fn test_checkpoint_to_tx_range() -> Result<()> {
    let (mut client, _emulator) = setup_emulator().await?;

    // Checkpoint 0: tx_lo=0, 10 transactions
    // Checkpoint 1: tx_lo=10, 5 transactions
    // Checkpoint 2: tx_lo=15, 20 transactions (network_total=35)
    // checkpoint_to_tx_range reads checkpoint summaries, so write those.
    // Checkpoint 0: 10 txns (network_total=10)
    // Checkpoint 1: 5 txns (network_total=15)
    // Checkpoint 2: 20 txns (network_total=35)
    write_checkpoint_summary(&mut client, 0, 10).await?;
    write_checkpoint_summary(&mut client, 1, 15).await?;
    write_checkpoint_summary(&mut client, 2, 35).await?;

    // Range [0, 3) → tx_seqs [0, 35)
    let range = client.checkpoint_to_tx_range(0..3).await?;
    assert_eq!(range, 0..35);

    // Range [1, 3) → tx_seqs [10, 35) — reads cp[0].network_total for start
    let range = client.checkpoint_to_tx_range(1..3).await?;
    assert_eq!(range, 10..35);

    // Range [0, 2) → tx_seqs [0, 15)
    let range = client.checkpoint_to_tx_range(0..2).await?;
    assert_eq!(range, 0..15);

    // Range [0, 1) → tx_seqs [0, 10)
    let range = client.checkpoint_to_tx_range(0..1).await?;
    assert_eq!(range, 0..10);

    Ok(())
}

#[tokio::test]
async fn test_resolve_tx_digests() -> Result<()> {
    let (mut client, _emulator) = setup_emulator().await?;

    // Checkpoint 0: tx_lo=0, 3 transactions
    // Checkpoint 1: tx_lo=3, 2 transactions
    let d0 = TransactionDigest::random();
    let d1 = TransactionDigest::random();
    let d2 = TransactionDigest::random();
    let d3 = TransactionDigest::random();
    let d4 = TransactionDigest::random();

    write_tx_seq_digests(&mut client, 0, &[d0, d1, d2], 0).await?;
    write_tx_seq_digests(&mut client, 3, &[d3, d4], 1).await?;

    // Resolve all 5
    let result = client.resolve_tx_digests(&[0, 1, 2, 3, 4]).await?;
    assert_eq!(result.len(), 5);
    assert_eq!(result[0], Some(tx_seq_digest_data(0, d0, 0)));
    assert_eq!(result[1], Some(tx_seq_digest_data(1, d1, 0)));
    assert_eq!(result[2], Some(tx_seq_digest_data(2, d2, 0)));
    assert_eq!(result[3], Some(tx_seq_digest_data(3, d3, 1)));
    assert_eq!(result[4], Some(tx_seq_digest_data(4, d4, 1)));

    // Resolve a subset
    let result = client.resolve_tx_digests(&[1, 4]).await?;
    assert_eq!(result.len(), 2);
    assert_eq!(result[0], Some(tx_seq_digest_data(1, d1, 0)));
    assert_eq!(result[1], Some(tx_seq_digest_data(4, d4, 1)));

    let mut checkpoints = client.resolve_tx_checkpoints(&[4, 0, 99]).await?;
    checkpoints.sort_by_key(|(tx_seq, _)| *tx_seq);
    assert_eq!(checkpoints, vec![(0, 0), (4, 1)]);

    // Unknown tx_seq → None
    let result = client.resolve_tx_digests(&[99]).await?;
    assert_eq!(result, vec![None]);

    // Empty input
    let result = client.resolve_tx_digests(&[]).await?;
    assert!(result.is_empty());

    Ok(())
}

#[tokio::test]
async fn test_full_pipeline() -> Result<()> {
    let (mut client, _emulator) = setup_emulator().await?;

    // Setup: 2 checkpoints
    // Checkpoint 0: 3 transactions (tx_seqs 0, 1, 2), network_total=3
    // Checkpoint 1: 2 transactions (tx_seqs 3, 4), network_total=5
    // Summaries for checkpoint_to_tx_range
    write_checkpoint_summary(&mut client, 0, 3).await?;
    write_checkpoint_summary(&mut client, 1, 5).await?;

    let sender_a = [0xAA; 32];
    let sender_b = [0xBB; 32];
    let dim_a = encode_dimension_key(IndexDimension::Sender, &sender_a);
    let dim_b = encode_dimension_key(IndexDimension::Sender, &sender_b);

    // sender_a sent tx_seqs 0, 2, 4
    write_bitmap(&mut client, &dim_a, 0, &[0, 2, 4]).await?;
    // sender_b sent tx_seqs 1, 3
    write_bitmap(&mut client, &dim_b, 0, &[1, 3]).await?;

    // Write checkpoint contents and transaction data for all 5
    let mut all_digests = Vec::new();
    for _ in 0..5u64 {
        all_digests.push(TransactionDigest::random());
    }
    write_tx_seq_digests(&mut client, 0, &all_digests[0..3], 0).await?;
    write_tx_seq_digests(&mut client, 3, &all_digests[3..5], 1).await?;
    for (i, d) in all_digests.iter().enumerate() {
        let cp = if i < 3 { 0u64 } else { 1u64 };
        write_transaction(&mut client, d, cp, 1000 * (cp + 1)).await?;
    }

    // Full pipeline: checkpoint range [0, 1] → tx_range → bitmap query → stream
    let tx_range = client.checkpoint_to_tx_range(0..2).await?;
    assert_eq!(tx_range, 0..5);

    // Query: sender_a only
    let query = BitmapQuery::scan(dim_a.clone())?;
    let matching: Vec<u64> = bitmap_query_stream(
        &client,
        query,
        tx_range.clone(),
        BitmapIndexSpec::tx(),
        ScanDirection::Ascending,
    )
    .try_collect()
    .await?;
    assert_eq!(matching, vec![0, 2, 4]);

    // Load those matching tx_seqs
    let fetched = client.get_transactions_for_seqs(matching, None).await?;

    assert_eq!(fetched.len(), 3);
    // Results may arrive in any order, so check by tx_seq.
    let by_seq: std::collections::HashMap<u64, _> = fetched.into_iter().collect();
    assert_eq!(by_seq[&0].digest, all_digests[0]);
    assert_eq!(by_seq[&2].digest, all_digests[2]);
    assert_eq!(by_seq[&4].digest, all_digests[4]);

    // Query: sender_a OR sender_b → all 5
    let query = BitmapQuery::new(vec![
        BitmapTerm::new(vec![BitmapLiteral::include(dim_a.clone())?])?,
        BitmapTerm::new(vec![BitmapLiteral::include(dim_b.clone())?])?,
    ])?;
    let matching: Vec<u64> = bitmap_query_stream(
        &client,
        query,
        tx_range.clone(),
        BitmapIndexSpec::tx(),
        ScanDirection::Ascending,
    )
    .try_collect()
    .await?;
    assert_eq!(matching, vec![0, 1, 2, 3, 4]);

    // Query: sender_a AND NOT sender_b → still [0, 2, 4] (no overlap)
    let query = BitmapQuery::new(vec![BitmapTerm::new(vec![
        BitmapLiteral::include(dim_a)?,
        BitmapLiteral::exclude(dim_b)?,
    ])?])?;
    let matching: Vec<u64> = bitmap_query_stream(
        &client,
        query,
        tx_range,
        BitmapIndexSpec::tx(),
        ScanDirection::Ascending,
    )
    .try_collect()
    .await?;
    assert_eq!(matching, vec![0, 2, 4]);

    Ok(())
}

#[test]
fn test_bitmap_query_rejects_invalid_dnf() -> Result<()> {
    let dim = encode_dimension_key(IndexDimension::Sender, &[0xCC; 32]);

    assert!(BitmapQuery::new(Vec::new()).is_err());
    assert!(BitmapTerm::new(vec![BitmapLiteral::exclude(dim)?]).is_err());

    Ok(())
}

/// resolve_tx_digests: 4 checkpoints, out-of-order input, exact boundaries, single tx_seq.
#[tokio::test]
async fn test_resolve_tx_digests_boundary_and_ordering() -> Result<()> {
    let (mut client, _emulator) = setup_emulator().await?;

    // cp0=[0,1,2], cp1=[3,4], cp2=[5,6,7,8], cp3=[9]
    let mut all_digests = Vec::new();
    for _ in 0..10 {
        all_digests.push(TransactionDigest::random());
    }

    write_tx_seq_digests(&mut client, 0, &all_digests[0..3], 0).await?;
    write_tx_seq_digests(&mut client, 3, &all_digests[3..5], 1).await?;
    write_tx_seq_digests(&mut client, 5, &all_digests[5..9], 2).await?;
    write_tx_seq_digests(&mut client, 9, &all_digests[9..10], 3).await?;

    // Out-of-order input — results must preserve input order.
    let result = client.resolve_tx_digests(&[9, 0, 4, 7, 2]).await?;
    assert_eq!(result.len(), 5);
    assert_eq!(result[0], Some(tx_seq_digest_data(9, all_digests[9], 3)));
    assert_eq!(result[1], Some(tx_seq_digest_data(0, all_digests[0], 0)));
    assert_eq!(result[2], Some(tx_seq_digest_data(4, all_digests[4], 1)));
    assert_eq!(result[3], Some(tx_seq_digest_data(7, all_digests[7], 2)));
    assert_eq!(result[4], Some(tx_seq_digest_data(2, all_digests[2], 0)));

    // tx_seqs at exact checkpoint boundaries (tx_lo values).
    let result = client.resolve_tx_digests(&[0, 3, 5, 9]).await?;
    assert_eq!(result.len(), 4);
    assert_eq!(result[0], Some(tx_seq_digest_data(0, all_digests[0], 0)));
    assert_eq!(result[1], Some(tx_seq_digest_data(3, all_digests[3], 1)));
    assert_eq!(result[2], Some(tx_seq_digest_data(5, all_digests[5], 2)));
    assert_eq!(result[3], Some(tx_seq_digest_data(9, all_digests[9], 3)));

    // Single tx_seq.
    let result = client.resolve_tx_digests(&[6]).await?;
    assert_eq!(result.len(), 1);
    assert_eq!(result[0], Some(tx_seq_digest_data(6, all_digests[6], 2)));

    Ok(())
}
