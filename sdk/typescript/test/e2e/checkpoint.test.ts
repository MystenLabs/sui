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
		const checkpointSequenceNumber = await toolbox.client.getLatestCheckpointSequenceNumber();
		expect(BigInt(checkpointSequenceNumber)).toBeGreaterThan(0);
	});

	it('gets checkpoint by id', async () => {
		const resp = await toolbox.client.getCheckpoint({ id: '0' });
		expect(resp.digest.length).greaterThan(0);
		expect(resp.transactions.length).greaterThan(0);
		expect(resp.epoch).not.toBeNull();
		expect(resp.sequenceNumber).not.toBeNull();
		expect(resp.networkTotalTransactions).not.toBeNull();
		expect(resp.epochRollingGasCostSummary).not.toBeNull();
		expect(resp.timestampMs).not.toBeNull();
	});

	it('get checkpoint contents by digest', async () => {
		const checkpoint_resp = await toolbox.client.getCheckpoint({ id: '0' });
		const digest = checkpoint_resp.digest;
		const resp = await toolbox.client.getCheckpoint({ id: digest });
		expect(checkpoint_resp).toEqual(resp);
	});

	it('getCheckpoints', async () => {
		const checkpoints = await toolbox.client.getCheckpoints({
			descendingOrder: false,
			limit: 1,
		});

		expect(checkpoints.nextCursor).toEqual('0');
		expect(checkpoints.data.length).toEqual(1);
		expect(checkpoints.hasNextPage).toBeTruthy();

		const checkpoints1 = await toolbox.client.getCheckpoints({
			cursor: checkpoints.nextCursor!,
			limit: 1,
			descendingOrder: false,
		});
		expect(checkpoints1.nextCursor).toEqual('1');
		expect(checkpoints1.data.length).toEqual(1);
		expect(checkpoints1.hasNextPage).toBeTruthy();
	});
});
