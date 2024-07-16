// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import path from 'path';
import { beforeAll, describe, expect, test } from 'vitest';

import { DeepBookClient } from '../src';
import { publishPackage, setupSuiClient, TestToolbox } from './setup';

let toolbox!: TestToolbox;

beforeAll(async () => {
	toolbox = await setupSuiClient();
	const tokenSourcesPath = path.join(__dirname, 'data/deepbook');
	await publishPackage(tokenSourcesPath, toolbox);
});

describe('DeepbookClient', () => {
	test('some test', async () => {
		const client = new DeepBookClient({
			address: toolbox.address(),
			env: 'testnet',
			client: toolbox.client,
		});

		expect(await client.getQuantityOut('DEEP_SUI', 1, 1)).toEqual(5);
	});
});
