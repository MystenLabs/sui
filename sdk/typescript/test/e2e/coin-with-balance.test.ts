// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import { fromHEX } from '@mysten/bcs';
import { beforeEach, describe, expect, it } from 'vitest';

import { bcs } from '../../src/bcs';
import { Ed25519Keypair } from '../../src/keypairs/ed25519';
import { TransactionBlock } from '../../src/transactions';
import { coinWithBalance } from '../../src/transactions/intents/CoinWithBalance';
import { publishPackage, setup, TestToolbox } from './utils/setup';

describe('coinWithBalance', () => {
	let toolbox: TestToolbox;
	let publishToolbox: TestToolbox;
	let packageId: string;
	let testType: string;

	beforeEach(async () => {
		[toolbox, publishToolbox] = await Promise.all([setup(), setup()]);
		const packagePath = __dirname + '/./data/coin_metadata';
		({ packageId } = await publishPackage(packagePath, publishToolbox));
		testType = packageId + '::test::TEST';
	});

	it('works with sui', async () => {
		const txb = new TransactionBlock();
		const receiver = new Ed25519Keypair();

		txb.transferObjects([coinWithBalance('0x2::sui::SUI', 12345n)], receiver.toSuiAddress());
		txb.setSender(publishToolbox.keypair.toSuiAddress());

		expect(
			JSON.parse(
				await txb.toJSON({
					supportedIntents: ['CoinWithBalance'],
				}),
			),
		).toEqual({
			expiration: null,
			gasData: {
				budget: null,
				owner: null,
				payment: null,
				price: null,
			},
			inputs: [
				{
					$kind: 'Pure',
					Pure: Array.from(fromHEX(receiver.toSuiAddress())),
				},
			],
			sender: publishToolbox.keypair.toSuiAddress(),
			transactions: [
				{
					$kind: 'Intent',
					Intent: {
						data: {
							balance: '12345',
							type: '0x2::sui::SUI',
						},
						inputs: {},
						name: 'CoinWithBalance',
					},
				},
				{
					$kind: 'TransferObjects',
					TransferObjects: {
						objects: [
							{
								$kind: 'Result',
								Result: 0,
							},
						],
						recipient: {
							$kind: 'Input',
							Input: 0,
							type: 'pure',
						},
					},
				},
			],
			version: 2,
		});

		expect(
			JSON.parse(
				await txb.toJSON({
					supportedIntents: [],
					client: toolbox.client,
				}),
			),
		).toEqual({
			expiration: null,
			gasData: {
				budget: null,
				owner: null,
				payment: null,
				price: null,
			},
			inputs: [
				{
					$kind: 'Pure',
					Pure: Array.from(fromHEX(receiver.toSuiAddress())),
				},
				{
					$kind: 'Pure',
					Pure: Array.from(bcs.u64().serialize(12345).toBytes()),
				},
			],
			sender: publishToolbox.keypair.toSuiAddress(),
			transactions: [
				{
					$kind: 'SplitCoins',
					SplitCoins: {
						coin: {
							$kind: 'GasCoin',
							GasCoin: true,
						},
						amounts: [
							{
								$kind: 'Input',
								Input: 1,
								type: 'pure',
							},
						],
					},
				},
				{
					$kind: 'TransferObjects',
					TransferObjects: {
						objects: [
							{
								$kind: 'NestedResult',
								NestedResult: [0, 0],
							},
						],
						recipient: {
							$kind: 'Input',
							Input: 0,
							type: 'pure',
						},
					},
				},
			],
			version: 2,
		});

		const result = await toolbox.client.signAndExecuteTransactionBlock({
			transactionBlock: txb,
			signer: publishToolbox.keypair,
			options: {
				showEffects: true,
				showBalanceChanges: true,
			},
		});

		expect(result.effects?.status.status).toBe('success');
		expect(
			result.balanceChanges?.find(
				(change) =>
					typeof change.owner === 'object' &&
					'AddressOwner' in change.owner &&
					change.owner.AddressOwner === receiver.toSuiAddress(),
			),
		).toEqual({
			amount: '12345',
			coinType: '0x2::sui::SUI',
			owner: {
				AddressOwner: receiver.toSuiAddress(),
			},
		});
	});

	it('works with custom coin', async () => {
		const txb = new TransactionBlock();
		const receiver = new Ed25519Keypair();

		txb.transferObjects([coinWithBalance(testType, 1n)], receiver.toSuiAddress());
		txb.setSender(publishToolbox.keypair.toSuiAddress());

		expect(
			JSON.parse(
				await txb.toJSON({
					supportedIntents: ['CoinWithBalance'],
				}),
			),
		).toEqual({
			expiration: null,
			gasData: {
				budget: null,
				owner: null,
				payment: null,
				price: null,
			},
			inputs: [
				{
					$kind: 'Pure',
					Pure: Array.from(fromHEX(receiver.toSuiAddress())),
				},
			],
			sender: publishToolbox.keypair.toSuiAddress(),
			transactions: [
				{
					$kind: 'Intent',
					Intent: {
						data: {
							balance: '1',
							type: testType,
						},
						inputs: {},
						name: 'CoinWithBalance',
					},
				},
				{
					$kind: 'TransferObjects',
					TransferObjects: {
						objects: [
							{
								$kind: 'Result',
								Result: 0,
							},
						],
						recipient: {
							$kind: 'Input',
							Input: 0,
							type: 'pure',
						},
					},
				},
			],
			version: 2,
		});

		expect(
			JSON.parse(
				await txb.toJSON({
					supportedIntents: [],
					client: publishToolbox.client,
				}),
			),
		).toEqual({
			expiration: null,
			gasData: {
				budget: null,
				owner: null,
				payment: null,
				price: null,
			},
			inputs: [
				{
					$kind: 'Pure',
					Pure: Array.from(fromHEX(receiver.toSuiAddress())),
				},
				{
					$kind: 'Object',
					Object: {
						$kind: 'ImmOrOwnedObject',
						ImmOrOwnedObject: expect.anything(),
					},
				},
				{
					$kind: 'Object',
					Object: {
						$kind: 'ImmOrOwnedObject',
						ImmOrOwnedObject: expect.anything(),
					},
				},
				{
					$kind: 'Pure',
					Pure: Array.from(bcs.u64().serialize(1).toBytes()),
				},
			],
			sender: publishToolbox.keypair.toSuiAddress(),
			transactions: [
				{
					$kind: 'MergeCoins',
					MergeCoins: {
						destination: {
							$kind: 'Input',
							Input: 1,
							type: 'object',
						},
						sources: [
							{
								$kind: 'Input',
								Input: 2,
								type: 'object',
							},
						],
					},
				},
				{
					$kind: 'SplitCoins',
					SplitCoins: {
						coin: {
							$kind: 'Input',
							Input: 1,
							type: 'object',
						},
						amounts: [
							{
								$kind: 'Input',
								Input: 3,
								type: 'pure',
							},
						],
					},
				},
				{
					$kind: 'TransferObjects',
					TransferObjects: {
						objects: [{ $kind: 'NestedResult', NestedResult: [1, 0] }],
						recipient: {
							$kind: 'Input',
							Input: 0,
							type: 'pure',
						},
					},
				},
			],
			version: 2,
		});

		const result = await toolbox.client.signAndExecuteTransactionBlock({
			transactionBlock: txb,
			signer: publishToolbox.keypair,
			options: {
				showEffects: true,
				showBalanceChanges: true,
			},
		});

		expect(result.effects?.status.status).toBe('success');
		expect(
			result.balanceChanges?.find(
				(change) =>
					typeof change.owner === 'object' &&
					'AddressOwner' in change.owner &&
					change.owner.AddressOwner === receiver.toSuiAddress(),
			),
		).toEqual({
			amount: '1',
			coinType: testType,
			owner: {
				AddressOwner: receiver.toSuiAddress(),
			},
		});
	});

	it('works with multiple coins', async () => {
		const txb = new TransactionBlock();
		const receiver = new Ed25519Keypair();

		txb.transferObjects(
			[
				coinWithBalance(testType, 1n),
				coinWithBalance(testType, 2n),
				coinWithBalance('0x2::sui::SUI', 3n),
				coinWithBalance('0x2::sui::SUI', 4n),
			],
			receiver.toSuiAddress(),
		);

		txb.setSender(publishToolbox.keypair.toSuiAddress());

		expect(
			JSON.parse(
				await txb.toJSON({
					supportedIntents: ['CoinWithBalance'],
				}),
			),
		).toEqual({
			expiration: null,
			gasData: {
				budget: null,
				owner: null,
				payment: null,
				price: null,
			},
			inputs: [
				{
					$kind: 'Pure',
					Pure: Array.from(fromHEX(receiver.toSuiAddress())),
				},
			],
			sender: publishToolbox.keypair.toSuiAddress(),
			transactions: [
				{
					$kind: 'Intent',
					Intent: {
						data: {
							balance: '1',
							type: testType,
						},
						inputs: {},
						name: 'CoinWithBalance',
					},
				},
				{
					$kind: 'Intent',
					Intent: {
						data: {
							balance: '2',
							type: testType,
						},
						inputs: {},
						name: 'CoinWithBalance',
					},
				},
				{
					$kind: 'Intent',
					Intent: {
						data: {
							balance: '3',
							type: '0x2::sui::SUI',
						},
						inputs: {},
						name: 'CoinWithBalance',
					},
				},
				{
					$kind: 'Intent',
					Intent: {
						data: {
							balance: '4',
							type: '0x2::sui::SUI',
						},
						inputs: {},
						name: 'CoinWithBalance',
					},
				},
				{
					$kind: 'TransferObjects',
					TransferObjects: {
						objects: [
							{
								$kind: 'Result',
								Result: 0,
							},
							{
								$kind: 'Result',
								Result: 1,
							},
							{
								$kind: 'Result',
								Result: 2,
							},
							{
								$kind: 'Result',
								Result: 3,
							},
						],
						recipient: {
							$kind: 'Input',
							Input: 0,
							type: 'pure',
						},
					},
				},
			],
			version: 2,
		});

		expect(
			JSON.parse(
				await txb.toJSON({
					supportedIntents: [],
					client: publishToolbox.client,
				}),
			),
		).toEqual({
			expiration: null,
			gasData: {
				budget: null,
				owner: null,
				payment: null,
				price: null,
			},
			inputs: [
				{
					$kind: 'Pure',
					Pure: Array.from(fromHEX(receiver.toSuiAddress())),
				},
				{
					$kind: 'Object',
					Object: {
						$kind: 'ImmOrOwnedObject',
						ImmOrOwnedObject: expect.anything(),
					},
				},
				{
					$kind: 'Object',
					Object: {
						$kind: 'ImmOrOwnedObject',
						ImmOrOwnedObject: expect.anything(),
					},
				},
				{
					$kind: 'Pure',
					Pure: Array.from(bcs.u64().serialize(1).toBytes()),
				},
				{
					$kind: 'Pure',
					Pure: Array.from(bcs.u64().serialize(2).toBytes()),
				},
				{
					$kind: 'Pure',
					Pure: Array.from(bcs.u64().serialize(3).toBytes()),
				},
				{
					$kind: 'Pure',
					Pure: Array.from(bcs.u64().serialize(4).toBytes()),
				},
			],
			sender: publishToolbox.keypair.toSuiAddress(),
			transactions: [
				{
					$kind: 'MergeCoins',
					MergeCoins: {
						destination: {
							$kind: 'Input',
							Input: 1,
							type: 'object',
						},
						sources: [
							{
								$kind: 'Input',
								Input: 2,
								type: 'object',
							},
						],
					},
				},
				{
					$kind: 'SplitCoins',
					SplitCoins: {
						coin: {
							$kind: 'Input',
							Input: 1,
							type: 'object',
						},
						amounts: [
							{
								$kind: 'Input',
								Input: 3,
								type: 'pure',
							},
						],
					},
				},
				{
					$kind: 'SplitCoins',
					SplitCoins: {
						coin: {
							$kind: 'Input',
							Input: 1,
							type: 'object',
						},
						amounts: [
							{
								$kind: 'Input',
								Input: 4,
								type: 'pure',
							},
						],
					},
				},
				{
					$kind: 'SplitCoins',
					SplitCoins: {
						coin: {
							$kind: 'GasCoin',
							GasCoin: true,
						},
						amounts: [
							{
								$kind: 'Input',
								Input: 5,
								type: 'pure',
							},
						],
					},
				},
				{
					$kind: 'SplitCoins',
					SplitCoins: {
						coin: {
							$kind: 'GasCoin',
							GasCoin: true,
						},
						amounts: [
							{
								$kind: 'Input',
								Input: 6,
								type: 'pure',
							},
						],
					},
				},
				{
					$kind: 'TransferObjects',
					TransferObjects: {
						objects: [
							{ $kind: 'NestedResult', NestedResult: [1, 0] },
							{ $kind: 'NestedResult', NestedResult: [2, 0] },
							{ $kind: 'NestedResult', NestedResult: [3, 0] },
							{ $kind: 'NestedResult', NestedResult: [4, 0] },
						],
						recipient: {
							$kind: 'Input',
							Input: 0,
							type: 'pure',
						},
					},
				},
			],
			version: 2,
		});

		const result = await toolbox.client.signAndExecuteTransactionBlock({
			transactionBlock: txb,
			signer: publishToolbox.keypair,
			options: {
				showEffects: true,
				showBalanceChanges: true,
			},
		});

		expect(result.effects?.status.status).toBe('success');
		expect(
			result.balanceChanges?.filter(
				(change) =>
					typeof change.owner === 'object' &&
					'AddressOwner' in change.owner &&
					change.owner.AddressOwner === receiver.toSuiAddress(),
			),
		).toEqual([
			{
				amount: '7',
				coinType: '0x2::sui::SUI',
				owner: {
					AddressOwner: receiver.toSuiAddress(),
				},
			},
			{
				amount: '3',
				coinType: testType,
				owner: {
					AddressOwner: receiver.toSuiAddress(),
				},
			},
		]);
	});
});
