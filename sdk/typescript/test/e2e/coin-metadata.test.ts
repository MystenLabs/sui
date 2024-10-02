// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import { resolve } from 'path';
import { beforeAll, describe, expect, it } from 'vitest';

import { setup, TestToolbox } from './utils/setup';

describe('Test Coin Metadata', () => {
	let toolbox: TestToolbox;
	let packageId: string;

	beforeAll(async () => {
		toolbox = await setup();
		const packagePath = resolve(__dirname, './data/coin_metadata');
		packageId = await toolbox.getPackage(packagePath);
	});

	it('Test accessing coin metadata', async () => {
		const coinMetadata = (await toolbox.client.getCoinMetadata({
			coinType: `${packageId}::test::TEST`,
		}))!;
		expect(coinMetadata.decimals).to.equal(2);
		expect(coinMetadata.name).to.equal('Test Coin');
		expect(coinMetadata.description).to.equal('Test coin metadata');
		expect(coinMetadata.iconUrl).to.equal('http://sui.io');
	});
});
