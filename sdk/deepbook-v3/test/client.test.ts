// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import { beforeAll, describe, expect, test } from 'vitest';
import { DeepBookClient, DeepBookConfig } from '../src';
import { CoinMap } from '../src/utils/constants';
import { publishCoins, publishDeepBook, setupSuiClient, TestToolbox } from './setup';

let toolbox!: TestToolbox;
let coins: CoinMap;
let deepbookPackageId: string;
let deepbookRegistryId: string;
let deepbookAdminCap: string;

beforeAll(async () => {
	toolbox = await setupSuiClient();
    coins = await publishCoins(toolbox);
    const res = await publishDeepBook(toolbox);
    deepbookPackageId = res.deepbookPackageId;
    deepbookRegistryId = res.deepbookRegistryId;
    deepbookAdminCap = res.deepbookAdminCap;
});

describe('DeepbookClient', () => {
	test('some test', async () => {
		const client = new DeepBookClient({
			address: toolbox.address(),
			env: 'testnet',
			client: toolbox.client,
		});
		const config = new DeepBookConfig({
			env: 'testnet',
			address: toolbox.address(),
			adminCap: deepbookAdminCap,
            coins: coins,
		})
		config.setPackageId(deepbookPackageId);
		config.setRegistryId(deepbookRegistryId);

		client.setConfig(config);
	});
});

describe('Should Deploy DeepBook', () => {
	test('some test', async () => {
		expect(5).toEqual(5);
	})
});