// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import { beforeAll, describe, expect, test } from 'vitest';

import { TransactionBlock } from '../../../src/builder';
import { SuiObjectData } from '../../../src/client';
import { publishPackage, setup, TestToolbox } from '../utils/setup';

describe('GraphQL SuiClient compatibility', () => {
	let toolbox: TestToolbox;
	let transactionBlockDigest: string;
	let packageId: string;
	let parentObjectId: string;

	beforeAll(async () => {
		toolbox = await setup({ rpcURL: 'http:127.0.0.1:9124' });

		const packagePath = __dirname + '/../data/dynamic_fields';
		({ packageId } = await publishPackage(packagePath, toolbox));

		await toolbox.client
			.getOwnedObjects({
				owner: toolbox.address(),
				options: { showType: true },
				filter: { StructType: `${packageId}::dynamic_fields_test::Test` },
			})
			.then(function (objects) {
				const data = objects.data[0].data as SuiObjectData;
				parentObjectId = data.objectId;
			});

		// create a simple transaction
		const txb = new TransactionBlock();
		const [coin] = txb.splitCoins(txb.gas, [1]);
		txb.transferObjects([coin], toolbox.address());
		const result = await toolbox.client.signAndExecuteTransactionBlock({
			transactionBlock: txb,
			signer: toolbox.keypair,
		});

		transactionBlockDigest = result.digest;

		await toolbox.client.waitForTransactionBlock({ digest: transactionBlockDigest });
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

	test.skip('queryTransactionBlocks', async () => {
		const { nextCursor: _, ...rpcTransactions } = await toolbox.client.queryTransactionBlocks({
			filter: {
				FromAddress: toolbox.address(),
			},
			options: {
				showBalanceChanges: true,
				showEffects: true,
				showEvents: true,
				showInput: true,
				showObjectChanges: true,
				showRawInput: true,
			},
		});

		const { nextCursor: __, ...graphQLTransactions } =
			await toolbox.graphQLClient!.queryTransactionBlocks({
				filter: {
					FromAddress: toolbox.address(),
				},
				options: {
					showBalanceChanges: true,
					showEffects: true,
					showEvents: true,
					showInput: true,
					showObjectChanges: true,
					showRawInput: true,
				},
			});

		expect(graphQLTransactions).toEqual(rpcTransactions);
	});

	test.skip('getTransactionBlock', async () => {
		const rpcTransactionBlock = await toolbox.client.getTransactionBlock({
			digest: transactionBlockDigest,
			options: {
				showBalanceChanges: true,
				showEffects: true,
				showEvents: true,
				showInput: true,
				showObjectChanges: true,
				showRawInput: true,
			},
		});
		const graphQLTransactionBlock = await toolbox.graphQLClient!.getTransactionBlock({
			digest: transactionBlockDigest,
			options: {
				showBalanceChanges: true,
				showEffects: true,
				showEvents: true,
				showInput: true,
				showObjectChanges: true,
				showRawInput: true,
			},
		});

		expect(graphQLTransactionBlock).toEqual(rpcTransactionBlock);
	});

	test.skip('multiGetTransactionBlocks', async () => {
		const [rpcTransactionBlock] = await toolbox.client.multiGetTransactionBlocks({
			digests: [transactionBlockDigest],
			options: {
				showBalanceChanges: true,
				showEffects: true,
				showEvents: true,
				showInput: true,
				showObjectChanges: true,
				showRawInput: true,
			},
		});
		const [graphQLTransactionBlock] = await toolbox.graphQLClient!.multiGetTransactionBlocks({
			digests: [transactionBlockDigest],
			options: {
				showBalanceChanges: true,
				showEffects: true,
				showEvents: true,
				showInput: true,
				showObjectChanges: true,
				showRawInput: true,
			},
		});

		expect(graphQLTransactionBlock).toEqual(rpcTransactionBlock);
	});

	test('getTotalTransactionBlocks', async () => {
		const rpc = await toolbox.client.getTotalTransactionBlocks();
		const graphql = await toolbox.graphQLClient!.getTotalTransactionBlocks();

		expect(graphql).toEqual(rpc);
	});

	test('getReferenceGasPrice', async () => {
		const rpc = await toolbox.client.getReferenceGasPrice();
		const graphql = await toolbox.graphQLClient!.getReferenceGasPrice();

		expect(graphql).toEqual(rpc);
	});

	test.skip('getStakes', async () => {
		// TODO: need to stake some coins first
		const rpc = await toolbox.client.getStakes({
			owner: toolbox.address(),
		});
		const graphql = await toolbox.graphQLClient!.getStakes({
			owner: toolbox.address(),
		});

		expect(graphql).toEqual(rpc);
	});

	test.skip('getStakesById', async () => {
		// TODO: need to stake some coins first
		const stakes = await toolbox.client.getStakes({
			owner: toolbox.address(),
		});
		const rpc = await toolbox.client.getStakesByIds({
			stakedSuiIds: [stakes[0].stakes[0].stakedSuiId],
		});
		const graphql = await toolbox.graphQLClient!.getStakesByIds({
			stakedSuiIds: [stakes[0].stakes[0].stakedSuiId],
		});

		expect(graphql).toEqual(rpc);
	});

	test.skip('getLatestSuiSystemState', async () => {
		const rpc = await toolbox.client.getLatestSuiSystemState();
		const graphql = await toolbox.graphQLClient!.getLatestSuiSystemState();

		expect(graphql).toEqual(rpc);
	});

	test.skip('queryEvents', async () => {
		const { nextCursor: _, ...rpc } = await toolbox.client.queryEvents({
			query: {
				Package: '0x3',
			},
			limit: 1,
		});

		const { nextCursor: __, ...graphql } = await toolbox.graphQLClient!.queryEvents({
			query: {
				Package: '0x3',
			},
			limit: 1,
		});

		expect(graphql).toEqual(rpc);
	});

	test.skip('devInspectTransactionBlock', async () => {
		const txb = new TransactionBlock();

		const rpc = await toolbox.client.devInspectTransactionBlock({
			transactionBlock: txb,
			sender: toolbox.address(),
		});

		const graphql = await toolbox.graphQLClient!.devInspectTransactionBlock({
			transactionBlock: txb,
			sender: toolbox.address(),
		});

		expect(graphql).toEqual(rpc);
	});

	test.skip('getDynamicFields', async () => {
		const rpc = await toolbox.client.getDynamicFields({
			parentId: parentObjectId,
		});

		const graphql = await toolbox.graphQLClient!.getDynamicFields({
			parentId: parentObjectId,
		});

		expect(graphql).toEqual(rpc);
	});

	test.skip('getDynamicFieldObject', async () => {
		const {
			data: [field],
		} = await toolbox.client.getDynamicFields({
			parentId: parentObjectId,
			limit: 1,
		});

		const rpc = await toolbox.client.getDynamicFieldObject({
			parentId: parentObjectId,
			name: field.name,
		});

		const graphql = await toolbox.graphQLClient!.getDynamicFieldObject({
			parentId: parentObjectId,
			// TODO: name in RPC has encoded value, which we can't encoded to BCS consistently
			name: {
				type: field.name.type,
				value: field.bcsName,
			},
		});

		expect(graphql).toEqual(rpc);
	});

	test.skip('subscribeEvent', async () => {
		// TODO
	});

	test.skip('subscribeTransaction', async () => {
		// TODO
	});

	test.skip('executeTransactionBlock', async () => {
		// TODO
	});
	test.skip('dryRunTransactionBlock', async () => {
		// TODO
	});

	test('getLatestCheckpointSequenceNumber', async () => {
		const rpc = await toolbox.client.getLatestCheckpointSequenceNumber();
		const graphql = await toolbox.graphQLClient!.getLatestCheckpointSequenceNumber();

		expect(graphql).toEqual(rpc);
	});

	test.only('getCheckpoint', async () => {
		const latest = await toolbox.client.getLatestCheckpointSequenceNumber();
		const rpc = await toolbox.client.getCheckpoint({
			id: latest,
		});
		const graphql = await toolbox.graphQLClient!.getCheckpoint({
			id: latest,
		});

		expect(graphql).toEqual(rpc);
	});
});
