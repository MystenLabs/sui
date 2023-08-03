// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import { describe, it, expect, beforeEach } from 'vitest';

import { TestToolbox, publishPackage, setup } from './setup';
import { DeepBook_sdk } from '../../src';
import { SUI_FRAMEWORK_ADDRESS, normalizeSuiObjectId } from '@mysten/sui.js/utils';

const DEFAULT_TICK_SIZE = 10000000;
const DEFAULT_LOT_SIZE = 10000;

describe('Create a pool', () => {
	let toolbox: TestToolbox;
	let packageId: string;

	beforeEach(async () => {
		toolbox = await setup();
		const packagePath = __dirname + '/./data/test_coin';
		({ packageId } = await publishPackage(packagePath));
	});

	it('test creating a pool', async () => {
		const toolbox = await setup();
		const deepbook = new DeepBook_sdk(toolbox.client, {
			pools: [],
			tokens: [],
			caps: [],
		});
        const baseToken = `${normalizeSuiObjectId(SUI_FRAMEWORK_ADDRESS)}::sui::SUI`;
        const quoteToken = `${packageId}::test::TEST`;
		const txb = deepbook.createPool(
			baseToken,
			quoteToken,
			DEFAULT_TICK_SIZE,
			DEFAULT_LOT_SIZE,
		);
		const resp = await toolbox.client.signAndExecuteTransactionBlock({
			signer: toolbox.keypair,
			transactionBlock: txb,
			options: {
				showEffects: true,
			},
		});
        expect(resp.effects?.status.status).toEqual('success');
	});
});
