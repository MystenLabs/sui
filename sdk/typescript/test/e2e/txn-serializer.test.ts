// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import { bcs } from '@mysten/bcs';
import { beforeAll, describe, expect, it } from 'vitest';

import { SuiTransactionBlockResponse } from '../../src/client';
import { Transaction } from '../../src/transactions';
import { TransactionDataBuilder } from '../../src/transactions/TransactionData';
import { SUI_SYSTEM_STATE_OBJECT_ID } from '../../src/utils';
import { publishPackage, setup, TestToolbox } from './utils/setup';

let toolbox: TestToolbox;
let packageId: string;
let publishTxn: SuiTransactionBlockResponse;
let sharedObjectId: string;
beforeAll(async () => {
	toolbox = await setup();
	const packagePath = __dirname + '/./data/serializer';
	({ packageId, publishTxn } = await publishPackage(packagePath));
	const sharedObject = publishTxn.effects?.created!.filter(
		(o) =>
			typeof o.owner === 'object' &&
			'Shared' in o.owner &&
			o.owner.Shared.initial_shared_version !== undefined,
	)[0];
	sharedObjectId = sharedObject!.reference.objectId;
});

describe('Transaction bcs Serialization and deserialization', () => {
	async function serializeAndDeserialize(tx: Transaction, mutable: boolean[]) {
		tx.setSender(await toolbox.address());
		const transactionBytes = await tx.build({
			client: toolbox.client,
		});
		const deserializedTxnBuilder = TransactionDataBuilder.fromBytes(transactionBytes);
		expect(
			deserializedTxnBuilder.inputs
				.filter((i) => i.Object?.SharedObject)
				.map((i) => i.Object?.SharedObject?.mutable),
		).toStrictEqual(mutable);
		const reserializedTxnBytes = await deserializedTxnBuilder.build();
		expect(reserializedTxnBytes).toEqual(transactionBytes);
	}

	it('Move Shared Object Call with mutable reference', async () => {
		const coins = await toolbox.getGasObjectsOwnedByAddress();

		const [{ suiAddress: validatorAddress }] = await toolbox.getActiveValidators();

		const tx = new Transaction();
		const coin = coins.data[2];
		tx.moveCall({
			target: '0x3::sui_system::request_add_stake',
			arguments: [
				tx.object(SUI_SYSTEM_STATE_OBJECT_ID),
				tx.object(coin.coinObjectId),
				tx.pure.address(validatorAddress),
			],
		});
		await serializeAndDeserialize(tx, [true]);
	});

	it('Move Shared Object Call with immutable reference', async () => {
		const tx = new Transaction();
		tx.moveCall({
			target: `${packageId}::serializer_tests::value`,
			arguments: [tx.object(sharedObjectId)],
		});
		await serializeAndDeserialize(tx, [false]);
	});

	it('Move Shared Object Call with mixed usage of mutable and immutable references', async () => {
		const tx = new Transaction();
		tx.moveCall({
			target: `${packageId}::serializer_tests::value`,
			arguments: [tx.object(sharedObjectId)],
		});
		tx.moveCall({
			target: `${packageId}::serializer_tests::set_value`,
			arguments: [tx.object(sharedObjectId)],
		});
		await serializeAndDeserialize(tx, [true]);
	});

	it('Transaction with expiration', async () => {
		const tx = new Transaction();
		tx.setExpiration({ Epoch: 100 });
		await serializeAndDeserialize(tx, []);
	});
});

describe('TXB v2 JSON serialization', () => {
	async function serializeAndDeserialize(tx: Transaction) {
		tx.setSender(await toolbox.address());
		tx.setGasOwner(await toolbox.address());
		tx.setExpiration({ None: true });
		tx.setSender(await toolbox.address());
		const transactionJson = await tx.getData();
		const deserializedTxnBuilder = Transaction.from(JSON.stringify(transactionJson));
		const reserializedTxnJson = await deserializedTxnBuilder.getData();
		expect(reserializedTxnJson).toEqual(transactionJson);
		const reserializedTxnBytes = await deserializedTxnBuilder.build({
			client: toolbox.client,
		});
		expect(reserializedTxnBytes).toEqual(
			await tx.build({
				client: toolbox.client,
			}),
		);

		expect(tx.getData()).toMatchObject(deserializedTxnBuilder.getData());
	}

	it('Move Shared Object Call with mutable reference', async () => {
		const coins = await toolbox.getGasObjectsOwnedByAddress();

		const [{ suiAddress: validatorAddress }] = await toolbox.getActiveValidators();

		const tx = new Transaction();
		const coin = coins.data[2];
		tx.moveCall({
			target: '0x3::sui_system::request_add_stake',
			arguments: [
				tx.object(SUI_SYSTEM_STATE_OBJECT_ID),
				tx.object(coin.coinObjectId),
				tx.pure.address(validatorAddress),
			],
		});
		await serializeAndDeserialize(tx);
	});

	it('serialized pure inputs', async () => {
		const [{ suiAddress: validatorAddress }] = await toolbox.getActiveValidators();

		const tx = new Transaction();

		tx.moveCall({
			target: `${packageId}::serializer_tests::addr`,
			arguments: [tx.pure.address(validatorAddress)],
		});
		tx.moveCall({
			target: `${packageId}::serializer_tests::id`,
			arguments: [tx.pure.id(validatorAddress)],
		});
		tx.moveCall({
			target: `${packageId}::serializer_tests::ascii_`,
			arguments: [tx.pure.string('hello')],
		});
		tx.moveCall({
			target: `${packageId}::serializer_tests::string`,
			arguments: [tx.pure.string('hello')],
		});
		tx.moveCall({
			target: `${packageId}::serializer_tests::vec`,
			arguments: [bcs.vector(bcs.string()).serialize(['hello'])],
		});
		tx.moveCall({
			target: `${packageId}::serializer_tests::opt`,
			arguments: [bcs.option(bcs.string()).serialize('hello')],
		});
		tx.moveCall({
			target: `${packageId}::serializer_tests::ints`,
			arguments: [
				tx.pure.u8(1),
				tx.pure.u16(2),
				tx.pure.u32(3),
				tx.pure.u64(4),
				tx.pure.u128(5),
				tx.pure.u256(6),
			],
		});
		tx.moveCall({
			target: `${packageId}::serializer_tests::boolean`,
			arguments: [tx.pure.bool(true)],
		});

		await serializeAndDeserialize(tx);
	});
});

describe('TXB v1 JSON serialization', () => {
	async function serializeAndDeserialize(tx: Transaction, json?: string) {
		tx.setSender(await toolbox.address());
		tx.setGasOwner(await toolbox.address());
		tx.setExpiration({ None: true });
		tx.setSender(await toolbox.address());
		const transactionJson = json ?? (await tx.serialize());
		const deserializedTxnBuilder = Transaction.from(transactionJson);
		const reserializedTxnBytes = await deserializedTxnBuilder.build({
			client: toolbox.client,
		});
		expect(reserializedTxnBytes).toEqual(
			await tx.build({
				client: toolbox.client,
			}),
		);

		const blockData = tx.getData();
		const blockDataFromJson = deserializedTxnBuilder.getData();

		if (json) {
			// Argument types aren't in v1 JSON
			blockDataFromJson.commands.forEach((txn) => {
				if (txn.MoveCall?._argumentTypes) {
					delete txn.MoveCall._argumentTypes;
				}
			});
		}

		expect(blockData).toMatchObject(blockDataFromJson);
	}

	it('Move Shared Object Call with mutable reference', async () => {
		const coins = await toolbox.getGasObjectsOwnedByAddress();

		const [{ suiAddress: validatorAddress }] = await toolbox.getActiveValidators();

		const tx = new Transaction();
		const coin = coins.data[2];
		tx.moveCall({
			target: '0x3::sui_system::request_add_stake',
			arguments: [
				tx.object(SUI_SYSTEM_STATE_OBJECT_ID),
				tx.object(coin.coinObjectId),
				tx.pure.address(validatorAddress),
			],
		});
		await serializeAndDeserialize(tx);
	});

	it('serializes pure inputs', async () => {
		const [{ suiAddress: validatorAddress }] = await toolbox.getActiveValidators();

		const tx = new Transaction();

		tx.moveCall({
			target: `${packageId}::serializer_tests::addr`,
			arguments: [tx.pure.address(validatorAddress)],
		});
		tx.moveCall({
			target: `${packageId}::serializer_tests::id`,
			arguments: [tx.pure.id(validatorAddress)],
		});
		tx.moveCall({
			target: `${packageId}::serializer_tests::ascii_`,
			arguments: [tx.pure.string('hello')],
		});
		tx.moveCall({
			target: `${packageId}::serializer_tests::string`,
			arguments: [tx.pure.string('hello')],
		});
		tx.moveCall({
			target: `${packageId}::serializer_tests::vec`,
			arguments: [bcs.vector(bcs.string()).serialize(['hello'])],
		});
		tx.moveCall({
			target: `${packageId}::serializer_tests::opt`,
			arguments: [bcs.option(bcs.string()).serialize('hello')],
		});
		tx.moveCall({
			target: `${packageId}::serializer_tests::ints`,
			arguments: [
				tx.pure.u8(1),
				tx.pure.u16(2),
				tx.pure.u32(3),
				tx.pure.u64(4),
				tx.pure.u128(5),
				tx.pure.u256(6),
			],
		});
		tx.moveCall({
			target: `${packageId}::serializer_tests::boolean`,
			arguments: [tx.pure.bool(true)],
		});

		await serializeAndDeserialize(tx);
	});

	it('parses raw values in pure inputs', async () => {
		const tx = new Transaction();

		tx.moveCall({
			target: `${packageId}::serializer_tests::addr`,
			arguments: [tx.pure.address(toolbox.address())],
		});
		tx.moveCall({
			target: `${packageId}::serializer_tests::id`,
			arguments: [tx.pure.id(toolbox.address())],
		});
		tx.moveCall({
			target: `${packageId}::serializer_tests::ascii_`,
			arguments: [tx.pure.string('hello')],
		});
		tx.moveCall({
			target: `${packageId}::serializer_tests::string`,
			arguments: [tx.pure.string('hello')],
		});
		tx.moveCall({
			target: `${packageId}::serializer_tests::vec`,
			arguments: [bcs.vector(bcs.string()).serialize(['hello'])],
		});
		tx.moveCall({
			target: `${packageId}::serializer_tests::opt`,
			arguments: [bcs.option(bcs.string()).serialize('hello')],
		});
		tx.moveCall({
			target: `${packageId}::serializer_tests::ints`,
			arguments: [
				tx.pure.u8(1),
				tx.pure.u16(2),
				tx.pure.u32(3),
				tx.pure.u64(4),
				tx.pure.u128(5),
				tx.pure.u256(6),
			],
		});
		tx.moveCall({
			target: `${packageId}::serializer_tests::boolean`,
			arguments: [tx.pure.bool(true)],
		});

		const v1JSON = {
			version: 1,
			sender: toolbox.address(),
			expiration: { None: true },
			gasConfig: { owner: toolbox.address() },
			inputs: [
				{
					kind: 'Input',
					index: 0,
					value: toolbox.address(),
					type: 'pure',
				},
				{
					kind: 'Input',
					index: 1,
					value: toolbox.address(),
					type: 'pure',
				},
				{ kind: 'Input', index: 2, value: 'hello', type: 'pure' },
				{ kind: 'Input', index: 3, value: 'hello', type: 'pure' },
				{ kind: 'Input', index: 4, value: ['hello'], type: 'pure' },
				{ kind: 'Input', index: 5, value: ['hello'], type: 'pure' },
				{ kind: 'Input', index: 6, value: 1, type: 'pure' },
				{ kind: 'Input', index: 7, value: 2, type: 'pure' },
				{ kind: 'Input', index: 8, value: 3, type: 'pure' },
				{ kind: 'Input', index: 9, value: 4, type: 'pure' },
				{
					kind: 'Input',
					index: 10,
					value: 5,
					type: 'pure',
				},
				{
					kind: 'Input',
					index: 11,
					value: 6,
					type: 'pure',
				},
				{ kind: 'Input', index: 12, value: true, type: 'pure' },
			],
			transactions: [
				{
					kind: 'MoveCall',
					target: `${packageId}::serializer_tests::addr`,
					typeArguments: [],
					arguments: [
						{
							kind: 'Input',
							index: 0,
						},
					],
				},
				{
					kind: 'MoveCall',
					target: `${packageId}::serializer_tests::id`,
					typeArguments: [],
					arguments: [
						{
							kind: 'Input',
							index: 1,
						},
					],
				},
				{
					kind: 'MoveCall',
					target: `${packageId}::serializer_tests::ascii_`,
					typeArguments: [],
					arguments: [
						{
							kind: 'Input',
							index: 2,
						},
					],
				},
				{
					kind: 'MoveCall',
					target: `${packageId}::serializer_tests::string`,
					typeArguments: [],
					arguments: [
						{
							kind: 'Input',
							index: 3,
						},
					],
				},
				{
					kind: 'MoveCall',
					target: `${packageId}::serializer_tests::vec`,
					typeArguments: [],
					arguments: [
						{
							kind: 'Input',
							index: 4,
						},
					],
				},
				{
					kind: 'MoveCall',
					target: `${packageId}::serializer_tests::opt`,
					typeArguments: [],
					arguments: [
						{
							kind: 'Input',
							index: 5,
						},
					],
				},
				{
					kind: 'MoveCall',
					target: `${packageId}::serializer_tests::ints`,
					typeArguments: [],
					arguments: [
						{ kind: 'Input', index: 6 },
						{ kind: 'Input', index: 7 },
						{ kind: 'Input', index: 8 },
						{ kind: 'Input', index: 9 },
						{
							kind: 'Input',
							index: 10,
						},
						{
							kind: 'Input',
							index: 11,
						},
					],
				},
				{
					kind: 'MoveCall',
					target: `${packageId}::serializer_tests::boolean`,
					typeArguments: [],
					arguments: [{ kind: 'Input', index: 12 }],
				},
			],
		};

		await serializeAndDeserialize(tx, JSON.stringify(v1JSON));
	});
});
