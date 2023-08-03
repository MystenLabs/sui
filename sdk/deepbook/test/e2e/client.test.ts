// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import { describe, it, expect, beforeEach } from 'vitest';

import { TestToolbox, setupSuiClient, setupPool } from './setup';
import { DeepBookClient } from '../../src';

describe('Deepbook client', () => {
	let toolbox: TestToolbox;

	beforeEach(async () => {
		toolbox = await setupSuiClient();
	});

	it('test getting all created pools', async () => {
		const pool = await setupPool(toolbox);

		const deepbook = new DeepBookClient(toolbox.client);
		const pools = await deepbook.getAllPools({});
		expect(pools.data.some((p) => p.poolId === pool.poolId)).toBeTruthy();
	});
});
