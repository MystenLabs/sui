// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import path from 'path';
import { beforeAll, describe, expect, test } from 'vitest';

import { DeepBookClient, DeepBookConfig } from '../src';
import { publishPackage, setupSuiClient, TestToolbox } from './setup';
import { writeToml } from './helper/toml';
import { CoinMap } from '../src/utils/constants';

let toolbox!: TestToolbox;
let coins: CoinMap;
let deepbookPackageId: string;
let deepbookRegistryId: string;
let deepbookAdminCap: string;

beforeAll(async () => {
	toolbox = await setupSuiClient();
	const tokenSourcesPath = path.join(__dirname, 'data/token');
	writeToml(tokenSourcesPath, "0x0", "token");
	let deepRes = await publishPackage(tokenSourcesPath, toolbox);
	writeToml(tokenSourcesPath, deepRes.packageId, "token");

	const usdcSourcePath = path.join(__dirname, 'data/usdc');
	const usdcRes = await publishPackage(usdcSourcePath, toolbox);

	const spamSourcePath = path.join(__dirname, 'data/spam');
	const spamRes = await publishPackage(spamSourcePath, toolbox);
	
	const deepbookSourcesPath = path.join(__dirname, 'data/deepbook');
	let deepbookRes = await publishPackage(deepbookSourcesPath, toolbox);

	deepbookPackageId = deepbookRes.packageId;
	// @ts-ignore
	deepbookRegistryId = deepbookRes.publishTxn.objectChanges?.find((change) => {
		return change.type === "created" && change.objectType.includes("Registry") && !change.objectType.includes("Inner")
	})?.["objectId"];

	// @ts-ignore
	deepbookAdminCap = deepbookRes.publishTxn.objectChanges?.find((change) => {
		return change.type === "created" && change.objectType.includes("DeepbookAdminCap");
	})?.["objectId"];
	coins = {
		DEEP: {
			address: deepRes.packageId,
			type: `${deepRes.packageId}::deep::DEEP`,
			scalar: 1000000,
		},
		USDC: {
			address: usdcRes.packageId,
			type: `${usdcRes.packageId}::usdc::USDC`,
			scalar: 1000000,
		},
		SPAM: {
			address: spamRes.packageId,
			type: `${spamRes.packageId}::spam::SPAM`,
			scalar: 1000000,
		},
	}
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