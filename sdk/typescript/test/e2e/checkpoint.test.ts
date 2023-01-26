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
        const checkpointSequenceNumber = await toolbox.provider.getLatestCheckpointSequenceNumber();
        expect(checkpointSequenceNumber).to.greaterThan(0);
    });

    it('Get checkpoint summary', async () => {
        const resp = await toolbox.provider.getCheckpointSummary(0);
        expect(resp.epoch).to.not.be.null;
        expect(resp.sequence_number).to.not.be.null;
        expect(resp.network_total_transactions).to.not.be.null;
        expect(resp.content_digest).to.not.be.null;
        expect(resp.epoch_rolling_gas_cost_summary).to.not.be.null;
        expect(resp.timestamp_ms).to.not.be.null;
    });

    it('get checkpoint summary by digest', async () => {
        const checkpoint_resp = await toolbox.provider.getCheckpointSummary(1);
        const digest = checkpoint_resp.previous_digest;
        expect(digest).to.not.be.null;
        const resp = await toolbox.provider.getCheckpointSummaryByDigest(digest!);
        expect(resp.epoch).to.not.be.null;
        expect(resp.sequence_number).to.not.be.null;
        expect(resp.network_total_transactions).to.not.be.null;
        expect(resp.content_digest).to.not.be.null;
        expect(resp.epoch_rolling_gas_cost_summary).to.not.be.null;
        expect(resp.timestamp_ms).to.not.be.null; 
    });

    it('get checkpoint contents', async () => {
        const resp = await toolbox.provider.getCheckpointContents(0);
        expect(resp.transactions.length).greaterThan(0);
    });

    it('get checkpoint contents by digest', async () => {
        const checkpoint_resp = await toolbox.provider.getCheckpointSummary(0);
        const digest = checkpoint_resp.content_digest;
        const resp = await toolbox.provider.getCheckpointContentsByDigest(digest);
        expect(resp.transactions.length).greaterThan(0);
    });
});
