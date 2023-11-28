// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import { beforeAll, describe, expect, test } from 'vitest';

import { publishPackage, setup, TestToolbox } from '../utils/setup';

describe('GraphQL SuiClient compatibility', () => {
	let toolbox: TestToolbox;

	beforeAll(async () => {
		toolbox = await setup({ rpcURL: 'http:127.0.0.1:9124' });
		const packagePath = __dirname + '/../data/dynamic_fields';
		await publishPackage(packagePath, toolbox);
	});

	test('getCoins', async () => {
		const rpcCoins = await toolbox.client.getCoins({
			owner: toolbox.address(),
		});
		const graphQLCoins = await toolbox.graphQLClient!.getCoins({
			owner: toolbox.address(),
		});

		expect(graphQLCoins).toEqual(rpcCoins);
	});

	test('getAllCoins', async () => {
		const rpcCoins = await toolbox.client.getAllCoins({
			owner: toolbox.address(),
		});
		const graphQLCoins = await toolbox.graphQLClient!.getAllCoins({
			owner: toolbox.address(),
		});

		expect(graphQLCoins).toEqual(rpcCoins);
	});

	test('getBalance', async () => {
		const rpcCoins = await toolbox.client.getBalance({
			owner: toolbox.address(),
		});
		const graphQLCoins = await toolbox.graphQLClient!.getBalance({
			owner: toolbox.address(),
		});

		expect(graphQLCoins).toEqual(rpcCoins);
	});
	test('getBalance', async () => {
		const rpcBalance = await toolbox.client.getBalance({
			owner: toolbox.address(),
		});
		const graphQLBalance = await toolbox.graphQLClient!.getBalance({
			owner: toolbox.address(),
		});

		expect(graphQLBalance).toEqual(rpcBalance);
	});

	test('getAllBalances', async () => {
		const rpcBalances = await toolbox.client.getAllBalances({
			owner: toolbox.address(),
		});
		const graphQLBalances = await toolbox.graphQLClient!.getAllBalances({
			owner: toolbox.address(),
		});

		expect(graphQLBalances).toEqual(rpcBalances);
	});

	test('getCoinMetadata', async () => {
		const rpcMetadata = await toolbox.client.getCoinMetadata({
			coinType: '0x02::sui::SUI',
		});

		const graphQLMetadata = await toolbox.graphQLClient!.getCoinMetadata({
			coinType: '0x02::sui::SUI',
		});

		expect(graphQLMetadata).toEqual(rpcMetadata);
	});

	test('getTotalSupply', async () => {
		const rpcSupply = await toolbox.client.getTotalSupply({
			coinType: '0x02::sui::SUI',
		});

		const graphQLgetTotalSupply = await toolbox.graphQLClient!.getTotalSupply({
			coinType: '0x02::sui::SUI',
		});

		expect(graphQLgetTotalSupply).toEqual(rpcSupply);
	});

	test.skip('getMoveFunctionArgTypes', async () => {
		const rpcMoveFunction = await toolbox.client.getMoveFunctionArgTypes({
			package: '0x02',
			module: 'coin',
			function: 'balance',
		});

		console.log(rpcMoveFunction);

		const graphQLMoveFunction = await toolbox.graphQLClient!.getMoveFunctionArgTypes({
			package: '0x02',
			module: 'coin',
			function: 'balance',
		});

		expect(graphQLMoveFunction).toEqual(rpcMoveFunction);
	});

	test.skip('getNormalizedMoveFunction', async () => {
		const rpcMoveFunction = await toolbox.client.getNormalizedMoveFunction({
			package: '0x02',
			module: 'coin',
			function: 'balance',
		});

		console.log(rpcMoveFunction);

		const graphQLMoveFunction = await toolbox.graphQLClient!.getNormalizedMoveFunction({
			package: '0x02',
			module: 'coin',
			function: 'balance',
		});

		expect(graphQLMoveFunction).toEqual(rpcMoveFunction);
	});

	test.skip('getNormalizedMoveModulesByPackage', async () => {
		const rpcMovePackage = await toolbox.client.getNormalizedMoveModulesByPackage({
			package: '0x02',
		});

		console.log(rpcMovePackage);

		const graphQLMovePackage = await toolbox.graphQLClient!.getNormalizedMoveModulesByPackage({
			package: '0x02',
		});

		expect(graphQLMovePackage).toEqual(rpcMovePackage);
	});

	test.skip('getNormalizedMoveModule', async () => {
		const rpcMoveModule = await toolbox.client.getNormalizedMoveModule({
			package: '0x02',
			module: 'coin',
		});

		console.log(rpcMoveModule);

		const graphQLMoveModule = await toolbox.graphQLClient!.getNormalizedMoveModule({
			package: '0x02',
			module: 'coin',
		});

		expect(graphQLMoveModule).toEqual(rpcMoveModule);
	});

	test.skip('getNormalizedMoveModule', async () => {
		const rpcMoveModule = await toolbox.client.getNormalizedMoveModule({
			package: '0x02',
			module: 'coin',
		});

		console.log(rpcMoveModule);

		const graphQLMoveModule = await toolbox.graphQLClient!.getNormalizedMoveModule({
			package: '0x02',
			module: 'coin',
		});

		expect(graphQLMoveModule).toEqual(rpcMoveModule);
	});

	test.skip('getNormalizedMoveStruct', async () => {
		const rpcMoveStruct = await toolbox.client.getNormalizedMoveStruct({
			package: '0x02',
			module: 'coin',
			struct: 'Balance',
		});

		console.log(rpcMoveStruct);

		const graphQLMoveStruct = await toolbox.graphQLClient!.getNormalizedMoveStruct({
			package: '0x02',
			module: 'coin',
			struct: 'Balance',
		});

		expect(graphQLMoveStruct).toEqual(rpcMoveStruct);
	});

	test.skip('getOwnedObjects', async () => {
		const rpcObjects = await toolbox.client.getOwnedObjects({
			owner: toolbox.address(),
			options: {
				showBcs: true,
				showContent: true,
				// showDisplay: true,
				showOwner: true,
				showPreviousTransaction: true,
				showStorageRebate: true,
				showType: true,
			},
		});
		const graphQLObjects = await toolbox.graphQLClient!.getOwnedObjects({
			owner: toolbox.address(),
			options: {
				showBcs: true,
				showContent: true,
				// showDisplay: true,
				showOwner: true,
				showPreviousTransaction: true,
				showStorageRebate: true,
				showType: true,
			},
		});

		expect(graphQLObjects).toEqual(rpcObjects);
	});

	test.skip('getObject', async () => {
		const {
			data: [{ coinObjectId: id }],
		} = await toolbox.getGasObjectsOwnedByAddress();

		const rpcObject = await toolbox.client.getObject({
			id,
			options: {
				showBcs: true,
				showContent: true,
				// showDisplay: true,
				showOwner: true,
				showPreviousTransaction: true,
				showStorageRebate: true,
				showType: true,
			},
		});
		const graphQLObject = await toolbox.graphQLClient!.getObject({
			id,
			options: {
				showBcs: true,
				showContent: true,
				// showDisplay: true,
				showOwner: true,
				showPreviousTransaction: true,
				showStorageRebate: true,
				showType: true,
			},
		});

		expect(graphQLObject).toEqual(rpcObject);
	});

	test.skip('tryGetPastObject', async () => {
		const {
			data: [{ coinObjectId: id, version }],
		} = await toolbox.getGasObjectsOwnedByAddress();

		const rpcObject = await toolbox.client.tryGetPastObject({
			id,
			version: Number.parseInt(version, 10),
			options: {
				showBcs: true,
				showContent: true,
				// showDisplay: true,
				showOwner: true,
				showPreviousTransaction: true,
				showStorageRebate: true,
				showType: true,
			},
		});
		const graphQLObject = await toolbox.graphQLClient!.tryGetPastObject({
			id,
			version: Number.parseInt(version, 10),
			options: {
				showBcs: true,
				showContent: true,
				// showDisplay: true,
				showOwner: true,
				showPreviousTransaction: true,
				showStorageRebate: true,
				showType: true,
			},
		});

		expect(graphQLObject).toEqual(rpcObject);
	});

	test.skip('multiGetObjects', async () => {
		const {
			data: [{ coinObjectId: id }],
		} = await toolbox.getGasObjectsOwnedByAddress();

		const rpcObjects = await toolbox.client.multiGetObjects({
			ids: [id],
			options: {
				showBcs: true,
				showContent: true,
				// showDisplay: true,
				showOwner: true,
				showPreviousTransaction: true,
				showStorageRebate: true,
				showType: true,
			},
		});
		const graphQLObjects = await toolbox.graphQLClient!.multiGetObjects({
			ids: [id],
			options: {
				showBcs: true,
				showContent: true,
				// showDisplay: true,
				showOwner: true,
				showPreviousTransaction: true,
				showStorageRebate: true,
				showType: true,
			},
		});

		expect(graphQLObjects).toEqual(rpcObjects);
	});

	test.only('queryTransactionBlocks', async () => {
		const { nextCursor: _, ...rpcTransactions } = await toolbox.client.queryTransactionBlocks({
			filter: {
				FromAddress: toolbox.address(),
			},
			options: {
				// showBalanceChanges: true,
				// showEffects: true,
				// showEvents: true,
				// showInput: true,
				// showObjectChanges: true,
				showRawInput: true,
			},
		});
		const { nextCursor: __, ...graphQLTransactions } =
			await toolbox.graphQLClient!.queryTransactionBlocks({
				filter: {
					FromAddress: toolbox.address(),
				},
				options: {
					// showBalanceChanges: true,
					// showEffects: true,
					// showEvents: true,
					// showInput: true,
					// showObjectChanges: true,
					showRawInput: true,
				},
			});

		console.log(graphQLTransactions);

		expect(graphQLTransactions).toEqual(rpcTransactions);
	});
});
