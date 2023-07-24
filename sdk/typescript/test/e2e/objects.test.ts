// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import { describe, it, expect, beforeAll } from 'vitest';
import { getObjectType, normalizeSuiAddress, SUI_TYPE_ARG, SuiObjectData } from '../../src';
import { TransactionBlock } from '../../src/builder';
import { setup, TestToolbox } from './utils/setup';

describe('Object Reading API', () => {
	let toolbox: TestToolbox;

	beforeAll(async () => {
		toolbox = await setup();
	});

	it('Get Owned Objects', async () => {
		const gasObjects = await toolbox.client.getOwnedObjects({
			owner: toolbox.address(),
		});
		expect(gasObjects.data.length).to.greaterThan(0);
	});

	it('Get Object', async () => {
		const gasObjects = await toolbox.getGasObjectsOwnedByAddress();
		expect(gasObjects.length).to.greaterThan(0);
		const objectInfos = await Promise.all(
			gasObjects.map((gasObject) => {
				const details = gasObject.data as SuiObjectData;
				return toolbox.client.getObject({
					id: details.objectId,
					options: { showType: true },
				});
			}),
		);
		objectInfos.forEach((objectInfo) =>
			expect(getObjectType(objectInfo)).to.equal('0x2::coin::Coin<0x2::sui::SUI>'),
		);
	});

	it('Get Objects', async () => {
		const gasObjects = await toolbox.getGasObjectsOwnedByAddress();
		expect(gasObjects.length).to.greaterThan(0);
		const gasObjectIds = gasObjects.map((gasObject) => {
			const details = gasObject.data as SuiObjectData;
			return details.objectId;
		});
		const objectInfos = await toolbox.client.multiGetObjects({
			ids: gasObjectIds,
			options: {
				showType: true,
			},
		});

		expect(gasObjects.length).to.equal(objectInfos.length);

		objectInfos.forEach((objectInfo) =>
			expect(getObjectType(objectInfo)).to.equal('0x2::coin::Coin<0x2::sui::SUI>'),
		);
	});

	it('handles trying to get non-existent old objects', async () => {
		const res = await toolbox.client.tryGetPastObject({
			id: normalizeSuiAddress('0x9999'),
			version: 0,
		});

		expect(res.status).toBe('ObjectNotExists');
	});

	it('can read live versions', async () => {
		const { data } = await toolbox.client.getCoins({
			owner: toolbox.address(),
			coinType: SUI_TYPE_ARG,
		});

		const res = await toolbox.client.tryGetPastObject({
			id: data[0].coinObjectId,
			version: Number(data[0].version),
		});

		expect(res.status).toBe('VersionFound');
	});

	it('handles trying to get a newer version than the latest version', async () => {
		const { data } = await toolbox.client.getCoins({
			owner: toolbox.address(),
			coinType: SUI_TYPE_ARG,
		});

		const res = await toolbox.client.tryGetPastObject({
			id: data[0].coinObjectId,
			version: Number(data[0].version) + 1,
		});

		expect(res.status).toBe('VersionTooHigh');
	});

	it('handles fetching versions that do not exist', async () => {
		const { data } = await toolbox.client.getCoins({
			owner: toolbox.address(),
			coinType: SUI_TYPE_ARG,
		});

		const res = await toolbox.client.tryGetPastObject({
			id: data[0].coinObjectId,
			// NOTE: This works because we know that this is a fresh coin that hasn't been modified:
			version: Number(data[0].version) - 1,
		});

		expect(res.status).toBe('VersionNotFound');
	});

	it('can find old versions of objects', async () => {
		const { data } = await toolbox.client.getCoins({
			owner: toolbox.address(),
			coinType: SUI_TYPE_ARG,
		});

		const tx = new TransactionBlock();
		// Transfer the entire gas object:
		tx.transferObjects([tx.gas], tx.pure(normalizeSuiAddress('0x2')));

		await toolbox.client.signAndExecuteTransactionBlock({
			signer: toolbox.keypair,
			transactionBlock: tx,
		});

		const res = await toolbox.client.tryGetPastObject({
			id: data[0].coinObjectId,
			// NOTE: This works because we know that this is a fresh coin that hasn't been modified:
			version: Number(data[0].version),
		});

		expect(res.status).toBe('VersionFound');
	});
});
