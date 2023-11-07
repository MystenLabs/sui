// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import { toB58 } from '@mysten/bcs';
import { expect, it } from 'vitest';

import { bcs } from '../../bcs/index.js';
import { normalizeSuiAddress } from '../../utils/sui-types.js';
import type { MoveCallTransaction, TransferObjectsTransaction } from '../index.js';
import { PROGRAMMABLE_CALL, TRANSACTION } from '../index.js';

// Oooh-weeee we nailed it!
it('can serialize simplified programmable call struct', () => {
	const moveCall: MoveCallTransaction = {
		kind: 'MoveCall',
		target: '0x2::display::new',
		typeArguments: ['0x6::capy::Capy'],
		arguments: [
			{ kind: 'GasCoin' },
			{
				kind: 'NestedResult',
				index: 0,
				resultIndex: 1,
			},
			{ kind: 'Input', index: 3 },
			{ kind: 'Result', index: 1 },
		],
	};

	const bytes = bcs.ser(PROGRAMMABLE_CALL, moveCall).toBytes();
	const result: MoveCallTransaction = bcs.de(PROGRAMMABLE_CALL, bytes);

	// since we normalize addresses when (de)serializing, the returned value differs
	// only check the module and the function; ignore address comparison (it's not an issue
	// with non-0x2 addresses).
	expect(result.arguments).toEqual(moveCall.arguments);
	expect(result.target.split('::').slice(1)).toEqual(moveCall.target.split('::').slice(1));
	expect(result.typeArguments[0].split('::').slice(1)).toEqual(
		moveCall.typeArguments[0].split('::').slice(1),
	);
});

it('can serialize enum with "kind" property', () => {
	const transaction: TransferObjectsTransaction = {
		kind: 'TransferObjects',
		objects: [],
		address: { kind: 'Input', index: 0 },
	};

	const bytes = bcs.ser(TRANSACTION, transaction).toBytes();
	const result: TransferObjectsTransaction = bcs.de(TRANSACTION, bytes);

	expect(result).toEqual(transaction);
});

it('can serialize Option<T> types using the legacy registry API', () => {
	const none = bcs.ser('Option<u8>', { None: true }).toBytes();
	const some = bcs.ser('Option<u8>', { Some: 2 }).toBytes();

	expect(none).toEqual(new Uint8Array([0]));
	expect(some).toEqual(new Uint8Array([1, 2]));
});

function ref(): { objectId: string; version: string; digest: string } {
	return {
		objectId: normalizeSuiAddress((Math.random() * 100000).toFixed(0).padEnd(64, '0')),
		version: String((Math.random() * 10000).toFixed(0)),
		digest: toB58(new Uint8Array([0, 1, 2, 3, 4, 5, 6, 7, 8, 9, 0, 1, 2, 3, 4, 5, 6, 7, 8, 9])),
	};
}

it('can serialize transaction data with a programmable transaction', () => {
	let sui = normalizeSuiAddress('0x2');
	let txData = {
		V1: {
			sender: normalizeSuiAddress('0xBAD'),
			expiration: { None: true },
			gasData: {
				payment: [ref()],
				owner: sui,
				price: '1',
				budget: '1000000',
			},
			kind: {
				ProgrammableTransaction: {
					inputs: [
						// first argument is the publisher object
						{ Object: { ImmOrOwned: ref() } },
						// second argument is a vector of names
						{
							Pure: Array.from(
								bcs.ser('vector<string>', ['name', 'description', 'img_url']).toBytes(),
							),
						},
						// third argument is a vector of values
						{
							Pure: Array.from(
								bcs
									.ser('vector<string>', [
										'Capy {name}',
										'A cute little creature',
										'https://api.capy.art/{id}/svg',
									])
									.toBytes(),
							),
						},
						// 4th and last argument is the account address to send display to
						{
							Pure: Array.from(bcs.ser('address', ref().objectId).toBytes()),
						},
					],
					transactions: [
						{
							kind: 'MoveCall',
							target: `${sui}::display::new`,
							typeArguments: [`${sui}::capy::Capy`],
							arguments: [
								// publisher object
								{ kind: 'Input', index: 0 },
							],
						},
						{
							kind: 'MoveCall',
							target: `${sui}::display::add_multiple`,
							typeArguments: [`${sui}::capy::Capy`],
							arguments: [
								// result of the first transaction
								{ kind: 'Result', index: 0 },
								// second argument - vector of names
								{ kind: 'Input', index: 1 },
								// third argument - vector of values
								{ kind: 'Input', index: 2 },
							],
						},
						{
							kind: 'MoveCall',
							target: `${sui}::display::update_version`,
							typeArguments: [`${sui}::capy::Capy`],
							arguments: [
								// result of the first transaction again
								{ kind: 'Result', index: 0 },
							],
						},
						{
							kind: 'TransferObjects',
							objects: [
								// the display object
								{ kind: 'Result', index: 0 },
							],
							// address is also an input
							address: { kind: 'Input', index: 3 },
						},
					],
				},
			},
		},
	};

	const type = 'TransactionData';
	const bytes = bcs.ser(type, txData).toBytes();
	const result = bcs.de(type, bytes);
	expect(result).toEqual(txData);
});
