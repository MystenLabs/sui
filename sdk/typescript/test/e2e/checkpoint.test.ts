// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import { describe, it, expect, beforeAll } from 'vitest';
import { setup, TestToolbox } from './utils/setup';

describe('Checkpoints Reading API', () => {
  let toolbox: TestToolbox;

  beforeAll(async () => {
    toolbox = await setup();
  });

  it('Get latest checkpoint sequence number', async () => {
    const checkpointSequenceNumber =
      await toolbox.provider.getLatestCheckpointSequenceNumber();
    expect(checkpointSequenceNumber).to.greaterThan(0);
  });

  it('Get checkpoint summary', async () => {
    const resp = await toolbox.provider.getCheckpointSummary(0);
    expect(resp.epoch).not.toBeNull();
    expect(resp.sequence_number).not.toBeNull();
    expect(resp.network_total_transactions).not.toBeNull();
    expect(resp.content_digest).not.toBeNull();
    expect(resp.epoch_rolling_gas_cost_summary).not.toBeNull();
    expect(resp.timestamp_ms).not.toBeNull();
  });

  it('get checkpoint summary by digest', async () => {
    const checkpoint_resp = await toolbox.provider.getCheckpointSummary(1);
    const digest = checkpoint_resp.previous_digest;
    expect(digest).not.toBeNull();
    const resp = await toolbox.provider.getCheckpointSummaryByDigest(digest!);
    expect(resp.epoch).not.toBeNull();
    expect(resp.sequence_number).not.toBeNull();
    expect(resp.network_total_transactions).not.toBeNull();
    expect(resp.content_digest).not.toBeNull();
    expect(resp.epoch_rolling_gas_cost_summary).not.toBeNull();
    expect(resp.timestamp_ms).not.toBeNull();
  });

  it('get checkpoint contents', async () => {
    const resp = await toolbox.provider.getCheckpointContents(0);
    expect(resp.transactions.length).greaterThan(0);
  });

  it('gets checkpoint by id', async () => {
    const resp = await toolbox.provider.getCheckpoint(0);
    expect(resp.digest.length).greaterThan(0);
  });

  it('get checkpoint contents by digest', async () => {
    const checkpoint_resp = await toolbox.provider.getCheckpointSummary(0);
    const digest = checkpoint_resp.content_digest;
    const resp = await toolbox.provider.getCheckpointContentsByDigest(digest);
    expect(resp.transactions.length).greaterThan(0);
  });
});
