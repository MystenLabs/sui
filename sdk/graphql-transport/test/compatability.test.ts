// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import { beforeAll, describe, expect, test } from 'vitest';

import {
	getFullnodeUrl,
	SuiClient,
	SuiObjectData,
	SuiTransactionBlockResponse,
} from '../../typescript/src/client/index.js';
import { Transaction } from '../../typescript/src/transactions/index.js';
import { publishPackage, setup, TestToolbox } from '../../typescript/test/e2e/utils/setup';
import { SuiClientGraphQLTransport } from '../src/transport';

const DEFAULT_GRAPHQL_URL = import.meta.env.DEFAULT_GRAPHQL_URL ?? 'http:127.0.0.1:9125';

describe('GraphQL SuiClient compatibility', () => {
	let toolbox: TestToolbox;
	let transactionBlockDigest: string;
	let packageId: string;
	let parentObjectId: string;
	const graphQLClient = new SuiClient({
		transport: new SuiClientGraphQLTransport({
			url: DEFAULT_GRAPHQL_URL,
			fallbackFullNodeUrl: getFullnodeUrl('localnet'),
		}),
	});

	beforeAll(async () => {
		toolbox = await setup({ rpcURL: 'http:127.0.0.1:9124' });

		const packagePath = __dirname + '/../../typescript/test/e2e/data/dynamic_fields';
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
		const tx = new Transaction();
		const [coin] = tx.splitCoins(tx.gas, [1]);
		tx.transferObjects([coin], toolbox.address());
		const result = await toolbox.client.signAndExecuteTransaction({
			transaction: tx as never,
			signer: toolbox.keypair,
		});

		transactionBlockDigest = result.digest;

		await toolbox.client.waitForTransaction({ digest: transactionBlockDigest });
		await graphQLClient.waitForTransaction({ digest: transactionBlockDigest });
	});

	test('getRpcApiVersion', async () => {
		const version = await graphQLClient!.getRpcApiVersion();

		// testing-no-sha is used for testing scenarios where we do not know the SHA
		expect(version?.match(/^\d+.\d+.\d+-(testing-no-sha|[a-z0-9]{40})$/)).not.toBeNull();
	});

	test('getCoins', async () => {
		const { data: rpcCoins } = await toolbox.client.getCoins({
			owner: toolbox.address(),
		});
		const { data: graphQLCoins } = await graphQLClient!.getCoins({
			owner: toolbox.address(),
		});

		expect(graphQLCoins).toEqual(rpcCoins);
	});

	test('getAllCoins', async () => {
		const { data: rpcCoins } = await toolbox.client.getAllCoins({
			owner: toolbox.address(),
		});
		const { data: graphQLCoins } = await graphQLClient!.getAllCoins({
			owner: toolbox.address(),
		});

		expect(graphQLCoins).toEqual(rpcCoins);
	});

	test('getBalance', async () => {
		const rpcCoins = await toolbox.client.getBalance({
			owner: toolbox.address(),
		});
		const graphQLCoins = await graphQLClient!.getBalance({
			owner: toolbox.address(),
		});

		expect(graphQLCoins).toEqual(rpcCoins);
	});
	test('getBalance', async () => {
		const rpcBalance = await toolbox.client.getBalance({
			owner: toolbox.address(),
		});
		const graphQLBalance = await graphQLClient!.getBalance({
			owner: toolbox.address(),
		});

		expect(graphQLBalance).toEqual(rpcBalance);
	});

	test('getAllBalances', async () => {
		const rpcBalances = await toolbox.client.getAllBalances({
			owner: toolbox.address(),
		});
		const graphQLBalances = await graphQLClient!.getAllBalances({
			owner: toolbox.address(),
		});

		expect(graphQLBalances).toEqual(rpcBalances);
	});

	test('getCoinMetadata', async () => {
		const rpcMetadata = await toolbox.client.getCoinMetadata({
			coinType: '0x02::sui::SUI',
		});

		const graphQLMetadata = await graphQLClient!.getCoinMetadata({
			coinType: '0x02::sui::SUI',
		});

		expect(graphQLMetadata).toEqual(rpcMetadata);
	});

	test('getTotalSupply', async () => {
		const rpcSupply = await toolbox.client.getTotalSupply({
			coinType: '0x02::sui::SUI',
		});

		const graphQLgetTotalSupply = await graphQLClient!.getTotalSupply({
			coinType: '0x02::sui::SUI',
		});

		expect(graphQLgetTotalSupply).toEqual(rpcSupply);
	});

	test('getMoveFunctionArgTypes', async () => {
		const rpcMoveFunction = await toolbox.client.getMoveFunctionArgTypes({
			package: '0x02',
			module: 'coin',
			function: 'balance',
		});

		const graphQLMoveFunction = await graphQLClient!.getMoveFunctionArgTypes({
			package: '0x02',
			module: 'coin',
			function: 'balance',
		});

		expect(graphQLMoveFunction).toEqual(rpcMoveFunction);
	});

	test('getNormalizedMoveFunction', async () => {
		const rpcMoveFunction = await toolbox.client.getNormalizedMoveFunction({
			package: '0x02',
			module: 'coin',
			function: 'balance',
		});

		const graphQLMoveFunction = await graphQLClient!.getNormalizedMoveFunction({
			package: '0x02',
			module: 'coin',
			function: 'balance',
		});

		expect(graphQLMoveFunction).toEqual(rpcMoveFunction);
	});

	test('getNormalizedMoveModulesByPackage', async () => {
		const rpcMovePackage = await toolbox.client.getNormalizedMoveModulesByPackage({
			package: '0x02',
		});

		const graphQLMovePackage = await graphQLClient!.getNormalizedMoveModulesByPackage({
			package: '0x02',
		});

		expect(graphQLMovePackage).toEqual(rpcMovePackage);
	});

	test('getNormalizedMoveModule', async () => {
		const rpcMoveModule = await toolbox.client.getNormalizedMoveModule({
			package: '0x02',
			module: 'coin',
		});

		const graphQLMoveModule = await graphQLClient!.getNormalizedMoveModule({
			package: '0x02',
			module: 'coin',
		});

		expect(graphQLMoveModule).toEqual(rpcMoveModule);
	});

	test('getNormalizedMoveStruct', async () => {
		const rpcMoveStruct = await toolbox.client.getNormalizedMoveStruct({
			package: '0x02',
			module: 'coin',
			struct: 'Coin',
		});

		const graphQLMoveStruct = await graphQLClient!.getNormalizedMoveStruct({
			package: '0x02',
			module: 'coin',
			struct: 'Coin',
		});

		expect(graphQLMoveStruct).toEqual(rpcMoveStruct);
	});

	test('getOwnedObjects', async () => {
		const { data: rpcObjects } = await toolbox.client.getOwnedObjects({
			owner: toolbox.address(),
			options: {
				showBcs: true,
				showContent: true,
				showDisplay: true,
				showOwner: true,
				showPreviousTransaction: true,
				showStorageRebate: true,
				showType: true,
			},
		});
		const { data: graphQLObjects } = await graphQLClient!.getOwnedObjects({
			owner: toolbox.address(),
			options: {
				showBcs: true,
				showContent: true,
				showDisplay: true,
				showOwner: true,
				showPreviousTransaction: true,
				showStorageRebate: true,
				showType: true,
			},
		});

		expect(graphQLObjects).toEqual(rpcObjects);
	});

	test('getObject', async () => {
		const {
			data: [{ coinObjectId: id }],
		} = await toolbox.getGasObjectsOwnedByAddress();

		const rpcObject = await toolbox.client.getObject({
			id,
			options: {
				showBcs: true,
				showContent: true,
				showDisplay: true,
				showOwner: true,
				showPreviousTransaction: true,
				showStorageRebate: true,
				showType: true,
			},
		});
		const graphQLObject = await graphQLClient!.getObject({
			id,
			options: {
				showBcs: true,
				showContent: true,
				showDisplay: true,
				showOwner: true,
				showPreviousTransaction: true,
				showStorageRebate: true,
				showType: true,
			},
		});

		expect(graphQLObject).toEqual(rpcObject);
	});

	test('tryGetPastObject', async () => {
		const {
			data: [{ coinObjectId: id, version }],
		} = await toolbox.getGasObjectsOwnedByAddress();
		const fullNodeClient = new SuiClient({
			url: getFullnodeUrl('localnet'),
		});

		const rpcObject = await fullNodeClient.tryGetPastObject({
			id,
			version: Number.parseInt(version, 10),
			options: {
				showBcs: true,
				showContent: true,
				showDisplay: true,
				showOwner: true,
				showPreviousTransaction: true,
				showStorageRebate: true,
				showType: true,
			},
		});
		const graphQLObject = await graphQLClient!.tryGetPastObject({
			id,
			version: Number.parseInt(version, 10),
			options: {
				showBcs: true,
				showContent: true,
				showDisplay: true,
				showOwner: true,
				showPreviousTransaction: true,
				showStorageRebate: true,
				showType: true,
			},
		});

		expect(graphQLObject).toEqual(rpcObject);
	});

	test('multiGetObjects', async () => {
		const {
			data: [{ coinObjectId: id }],
		} = await toolbox.getGasObjectsOwnedByAddress();

		const rpcObjects = await toolbox.client.multiGetObjects({
			ids: [id],
			options: {
				showBcs: true,
				showContent: true,
				showDisplay: true,
				showOwner: true,
				showPreviousTransaction: true,
				showStorageRebate: true,
				showType: true,
			},
		});
		const graphQLObjects = await graphQLClient!.multiGetObjects({
			ids: [id],
			options: {
				showBcs: true,
				showContent: true,
				showDisplay: true,
				showOwner: true,
				showPreviousTransaction: true,
				showStorageRebate: true,
				showType: true,
			},
		});

		expect(graphQLObjects).toEqual(rpcObjects);
	});

	test('queryTransactionBlocks', async () => {
		const { nextCursor: _, ...rpcTransactions } = await toolbox.client.queryTransactionBlocks({
			filter: {
				FromAddress: toolbox.address(),
			},
			options: {
				showBalanceChanges: true,
				showEffects: true,
				showRawEffects: true,
				showEvents: true,
				// TODO inputs missing valueType
				showInput: false,
				showObjectChanges: true,
				showRawInput: true,
			},
		});

		const { nextCursor: __, ...graphQLTransactions } = await graphQLClient!.queryTransactionBlocks({
			filter: {
				FromAddress: toolbox.address(),
			},
			options: {
				showBalanceChanges: true,
				showEffects: true,
				showRawEffects: true,
				showEvents: true,
				// TODO inputs missing valueType
				showInput: false,
				showObjectChanges: true,
				showRawInput: true,
			},
		});

		expect(graphQLTransactions).toEqual(rpcTransactions);
	});

	test('getTransactionBlock', async () => {
		const { rawEffects, ...rpcTransactionBlock } = (await toolbox.client.getTransactionBlock({
			digest: transactionBlockDigest,
			options: {
				showBalanceChanges: true,
				showEffects: true,
				showEvents: true,
				// TODO inputs missing valueType
				showInput: false,
				showObjectChanges: true,
				showRawInput: true,
			},
		})) as SuiTransactionBlockResponse & { rawEffects: unknown };
		const graphQLTransactionBlock = await graphQLClient!.getTransactionBlock({
			digest: transactionBlockDigest,
			options: {
				showBalanceChanges: true,
				showEffects: true,
				// TODO inputs missing valueType
				showInput: false,
				showObjectChanges: true,
				showRawInput: true,
			},
		});

		expect(graphQLTransactionBlock).toEqual(rpcTransactionBlock);
	});

	test('multiGetTransactionBlocks', async () => {
		const [rpcTransactionBlock] = await toolbox.client.multiGetTransactionBlocks({
			digests: [transactionBlockDigest],
			options: {
				showBalanceChanges: true,
				showEffects: true,
				showEvents: true,
				showRawEffects: true,
				// TODO inputs missing valueType
				showInput: false,
				showObjectChanges: true,
				showRawInput: true,
			},
		});
		const [graphQLTransactionBlock] = await graphQLClient!.multiGetTransactionBlocks({
			digests: [transactionBlockDigest],
			options: {
				showBalanceChanges: true,
				showEffects: true,
				showRawEffects: true,
				showEvents: true,
				// TODO inputs missing valueType
				showInput: false,
				showObjectChanges: true,
				showRawInput: true,
			},
		});

		expect(graphQLTransactionBlock).toEqual(rpcTransactionBlock);
	});

	test('getTotalTransactionBlocks', async () => {
		const rpc = await toolbox.client.getTotalTransactionBlocks();
		const graphql = await graphQLClient!.getTotalTransactionBlocks();

		expect(Number(graphql)).closeTo(Number(rpc), 10);
	});

	test('getReferenceGasPrice', async () => {
		const rpc = await toolbox.client.getReferenceGasPrice();
		const graphql = await graphQLClient!.getReferenceGasPrice();

		expect(graphql).toEqual(rpc);
	});

	test('getStakes', async () => {
		const rpc = await toolbox.client.getStakes({
			owner: toolbox.address(),
		});
		const graphql = await graphQLClient!.getStakes({
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
		const graphql = await graphQLClient!.getStakesByIds({
			stakedSuiIds: [stakes[0].stakes[0].stakedSuiId],
		});

		expect(graphql).toEqual(rpc);
	});

	test.skip('getLatestSuiSystemState', async () => {
		const rpc = await toolbox.client.getLatestSuiSystemState();
		const graphql = await graphQLClient!.getLatestSuiSystemState();

		expect(graphql).toEqual(rpc);
	});

	test.skip('queryEvents', async () => {
		const { nextCursor: _, ...rpc } = await toolbox.client.queryEvents({
			query: {
				Package: '0x3',
			},
			limit: 1,
		});

		const { nextCursor: __, ...graphql } = await graphQLClient!.queryEvents({
			query: {
				Package: '0x3',
			},
			limit: 1,
		});

		expect(graphql).toEqual(rpc);
	});

	test('devInspectTransactionBlock', async () => {
		const tx = new Transaction();
		tx.setSender(toolbox.address());
		const [coin] = tx.splitCoins(tx.gas, [1]);
		tx.transferObjects([coin], toolbox.address());

		const { effects, results, ...rpc } = await toolbox.client.devInspectTransactionBlock({
			transactionBlock: tx as never,
			sender: toolbox.address(),
		});

		const {
			effects: _,
			results: __,
			...graphql
		} = await graphQLClient!.devInspectTransactionBlock({
			transactionBlock: tx,
			sender: toolbox.address(),
		});

		expect(graphql).toEqual(rpc);
	});

	test.skip('getDynamicFields', async () => {
		const { nextCursor, ...rpc } = await toolbox.client.getDynamicFields({
			parentId: parentObjectId,
		});

		const { nextCursor: _, ...graphql } = await graphQLClient!.getDynamicFields({
			parentId: parentObjectId,
		});

		expect(graphql).toEqual(rpc);
	});

	test('getDynamicFieldObject', async () => {
		const { data } = await toolbox.client.getDynamicFields({
			parentId: parentObjectId,
		});

		const field = data.find((field) => field.type === 'DynamicObject')!;

		const rpc = await toolbox.client.getDynamicFieldObject({
			parentId: parentObjectId,
			name: field.name,
		});

		const graphql = await graphQLClient!.getDynamicFieldObject({
			parentId: parentObjectId,
			name: field.name,
		});

		expect(graphql).toEqual(rpc);
	});

	test.skip('subscribeEvent', async () => {
		// TODO
	});

	test.skip('subscribeTransaction', async () => {
		// TODO
	});

	test('executeTransactionBlock', async () => {
		const tx = new Transaction();
		tx.setSender(toolbox.address());
		const [coin] = tx.splitCoins(tx.gas, [1]);
		tx.transferObjects([coin], toolbox.address());

		const { confirmedLocalExecution, ...graphql } = await graphQLClient!.signAndExecuteTransaction({
			transaction: tx as Transaction,
			signer: toolbox.keypair,
			options: {
				showBalanceChanges: true,
				showEffects: true,
				showEvents: true,
				showInput: true,
				showObjectChanges: true,
				showRawInput: true,
			},
		});

		await toolbox.client.waitForTransaction({ digest: graphql.digest });

		const { checkpoint, timestampMs, rawEffects, ...rpc } =
			(await toolbox.client.getTransactionBlock({
				digest: graphql.digest,
				options: {
					showBalanceChanges: true,
					showEffects: true,
					showEvents: true,
					showInput: true,
					showObjectChanges: true,
					showRawInput: true,
				},
			})) as SuiTransactionBlockResponse & { rawEffects: unknown };

		expect(graphql).toEqual(rpc);
	});

	test('dryRunTransactionBlock', async () => {
		const tx = new Transaction();
		tx.setSender(toolbox.address());
		const [coin] = tx.splitCoins(tx.gas, [1]);
		tx.transferObjects([coin], toolbox.address());
		const bytes = await tx.build({ client: toolbox.client as never });

		const rpc = await toolbox.client.dryRunTransactionBlock({
			transactionBlock: bytes,
		});

		const graphql = await graphQLClient!.dryRunTransactionBlock({
			transactionBlock: bytes,
		});

		expect(graphql).toEqual(rpc);
	});

	test('getLatestCheckpointSequenceNumber', async () => {
		const rpc = await toolbox.client.getLatestCheckpointSequenceNumber();
		const graphql = await graphQLClient!.getLatestCheckpointSequenceNumber();

		expect(Number.parseInt(graphql)).closeTo(Number.parseInt(rpc), 3);
	});

	test('getCheckpoint', async () => {
		const rpc = await toolbox.client.getCheckpoint({
			id: '3',
		});
		const graphql = await graphQLClient!.getCheckpoint({
			id: '3',
		});

		expect(graphql).toEqual(rpc);
	});

	test('getCheckpoints', async () => {
		const { data: rpc } = await toolbox.client.getCheckpoints({
			descendingOrder: false,
			limit: 5,
		});
		const { data: graphql } = await graphQLClient!.getCheckpoints({
			descendingOrder: false,
			limit: 5,
		});

		expect(graphql).toEqual(rpc);
	});

	test.skip('getCommitteeInfo', async () => {
		const rpc = await toolbox.client.getCommitteeInfo({});
		const graphql = await graphQLClient!.getCommitteeInfo({});

		expect(graphql).toEqual(rpc);
	});

	test.skip('getNetworkMetrics', async () => {
		const rpc = await toolbox.client.getNetworkMetrics();
		const graphql = await graphQLClient!.getNetworkMetrics();

		expect(graphql).toEqual(rpc);
	});

	test.skip('getMoveCallMetrics', async () => {
		const rpc = await toolbox.client.getMoveCallMetrics();
		const graphql = await graphQLClient!.getMoveCallMetrics();

		expect(graphql).toEqual(rpc);
	});

	test.skip('getAddressMetrics', async () => {
		const rpc = await toolbox.client.getAddressMetrics();
		const graphql = await graphQLClient!.getAddressMetrics();

		expect(graphql).toEqual(rpc);
	});

	test.skip('getAllEpochAddressMetrics', async () => {
		const rpc = await toolbox.client.getAllEpochAddressMetrics();
		const graphql = await graphQLClient!.getAllEpochAddressMetrics();

		expect(graphql).toEqual(rpc);
	});

	test.skip('getEpochs', async () => {
		const rpc = await toolbox.client.getEpochs();
		const graphql = await graphQLClient!.getEpochs();

		expect(graphql).toEqual(rpc);
	});

	test.skip('getCurrentEpoch', async () => {
		const rpc = await toolbox.client.getCurrentEpoch();
		const graphql = await graphQLClient!.getCurrentEpoch();

		expect(graphql).toEqual(rpc);
	});

	test.skip('getValidatorsApy', async () => {
		const rpc = await toolbox.client.getValidatorsApy();
		const graphql = await graphQLClient!.getValidatorsApy();

		for (let i = 0; i < rpc.apys.length; i++) {
			expect(graphql.apys[i].address).toEqual(rpc.apys[i].address);
		}
	});

	test('getChainIdentifier', async () => {
		const rpc = await toolbox.client.getChainIdentifier();
		const graphql = await graphQLClient!.getChainIdentifier();

		expect(graphql).toEqual(rpc);
	});

	test('getProtocolConfig', async () => {
		const rpc = await toolbox.client.getProtocolConfig();
		const graphql = await graphQLClient!.getProtocolConfig();

		expect(graphql).toEqual(rpc);
	});

	test('resolveNameServiceAddress', async () => {
		const rpc = await toolbox.client.resolveNameServiceAddress({
			name: 'test.sui',
		});
		const graphql = await graphQLClient!.resolveNameServiceAddress({
			name: 'test.sui',
		});

		expect(graphql).toEqual(rpc);
	});

	test('resolveNameServiceNames', async () => {
		const rpc = await toolbox.client.resolveNameServiceNames({
			address: toolbox.address(),
		});
		const graphql = await graphQLClient!.resolveNameServiceNames({
			address: toolbox.address(),
		});

		expect(graphql).toEqual(rpc);
	});
});
