// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import { toB58 } from '@mysten/bcs';
import { describe, expect, it } from 'vitest';

import { bcs } from '../../bcs/index.js';
import { TransactionBlock, Transactions } from '../index.js';
import { Inputs } from '../Inputs.js';

it('can construct and serialize an empty tranaction', () => {
	const tx = new TransactionBlock();
	expect(() => tx.serialize()).not.toThrow();
});

it('can construct a receiving transaction argument', () => {
	const tx = new TransactionBlock();
	tx.object(Inputs.ReceivingRef(ref()));
	expect(() => tx.serialize()).not.toThrow();
});

it('receiving transaction argument different from object argument', () => {
	const oref = ref();
	const rtx = new TransactionBlock();
	rtx.object(Inputs.ReceivingRef(oref));
	const otx = new TransactionBlock();
	otx.object(Inputs.ObjectRef(oref));
	expect(() => rtx.serialize()).not.toThrow();
	expect(() => otx.serialize()).not.toThrow();
	expect(otx.serialize()).not.toEqual(rtx.serialize());
});

it('can be serialized and deserialized to the same values', () => {
	const tx = new TransactionBlock();
	tx.add(Transactions.SplitCoins(tx.gas, [tx.pure.u64(100)]));
	const serialized = tx.serialize();
	const tx2 = TransactionBlock.from(serialized);
	expect(serialized).toEqual(tx2.serialize());
});

it('allows transfer with the result of split transactions', () => {
	const tx = new TransactionBlock();
	const coin = tx.add(Transactions.SplitCoins(tx.gas, [tx.pure.u64(100)]));
	tx.add(Transactions.TransferObjects([coin], tx.object('0x2')));
});

it('supports nested results through either array index or destructuring', () => {
	const tx = new TransactionBlock();
	const registerResult = tx.add(
		Transactions.MoveCall({
			target: '0x2::game::register',
		}),
	);

	const [nft, account] = registerResult;

	// NOTE: This might seem silly but destructuring works differently than property access.
	expect(nft).toBe(registerResult[0]);
	expect(account).toBe(registerResult[1]);
});

describe('offline build', () => {
	it('builds an empty transaction offline when provided sufficient data', async () => {
		const tx = setup();
		await tx.build();
	});

	it('supports epoch expiration', async () => {
		const tx = setup();
		tx.setExpiration({ Epoch: 1 });
		await tx.build();
	});

	it('builds a split transaction', async () => {
		const tx = setup();
		tx.add(Transactions.SplitCoins(tx.gas, [tx.pure(Inputs.Pure(100, 'u64'))]));
		await tx.build();
	});

	it('breaks reference equality', () => {
		const tx = setup();
		const tx2 = new TransactionBlock(tx);

		tx.setGasBudget(999);

		// Ensure that setting budget after a clone does not affect the original:
		expect(tx2.blockData).not.toEqual(tx.blockData);

		// Ensure `blockData` always breaks reference equality:
		expect(tx.blockData).not.toBe(tx.blockData);
		expect(tx.blockData.gasConfig).not.toBe(tx.blockData.gasConfig);
		expect(tx.blockData.transactions).not.toBe(tx.blockData.transactions);
		expect(tx.blockData.inputs).not.toBe(tx.blockData.inputs);
	});

	it('can determine the type of inputs for built-in transactions', async () => {
		const tx = setup();
		tx.add(Transactions.SplitCoins(tx.gas, [tx.pure(100)]));
		await tx.build();
	});

	it('supports pre-serialized inputs as Uint8Array', async () => {
		const tx = setup();
		const inputBytes = bcs.ser('u64', 100n).toBytes();
		// Use bytes directly in pure value:
		tx.add(Transactions.SplitCoins(tx.gas, [tx.pure(inputBytes)]));
		// Use bytes in input helper:
		tx.add(Transactions.SplitCoins(tx.gas, [tx.pure(Inputs.Pure(inputBytes))]));
		await tx.build();
	});

	it('builds a more complex interaction', async () => {
		const tx = setup();
		const coin = tx.splitCoins(tx.gas, [100]);
		tx.add(Transactions.MergeCoins(tx.gas, [coin, tx.object(Inputs.ObjectRef(ref()))]));
		tx.add(
			Transactions.MoveCall({
				target: '0x2::devnet_nft::mint',
				typeArguments: [],
				arguments: [tx.pure.string('foo'), tx.pure.string('bar'), tx.pure.string('baz')],
			}),
		);
		await tx.build();
	});

	it('uses a receiving argument', async () => {
		const tx = setup();
		tx.object(Inputs.ObjectRef(ref()));
		const coin = tx.add(Transactions.SplitCoins(tx.gas, [tx.pure(100)]));
		tx.add(Transactions.MergeCoins(tx.gas, [coin, tx.object(Inputs.ObjectRef(ref()))]));
		tx.add(
			Transactions.MoveCall({
				target: '0x2::devnet_nft::mint',
				typeArguments: [],
				arguments: [tx.object(Inputs.ObjectRef(ref())), tx.object(Inputs.ReceivingRef(ref()))],
			}),
		);

		const bytes = await tx.build();
		const tx2 = TransactionBlock.from(bytes);
		const bytes2 = await tx2.build();

		expect(bytes).toEqual(bytes2);
	});

	it('builds a more complex interaction', async () => {
		const tx = setup();
		const coin = tx.splitCoins(tx.gas, [100]);
		tx.add(Transactions.MergeCoins(tx.gas, [coin, tx.object(Inputs.ObjectRef(ref()))]));
		tx.add(
			Transactions.MoveCall({
				target: '0x2::devnet_nft::mint',
				typeArguments: [],
				arguments: [tx.pure.string('foo'), tx.pure.string('bar'), tx.pure.string('baz')],
			}),
		);

		const bytes = await tx.build();
		const tx2 = TransactionBlock.from(bytes);
		const bytes2 = await tx2.build();

		expect(bytes).toEqual(bytes2);
	});
});

function ref(): { objectId: string; version: string; digest: string } {
	return {
		objectId: (Math.random() * 100000).toFixed(0).padEnd(64, '0'),
		version: String((Math.random() * 10000).toFixed(0)),
		digest: toB58(new Uint8Array([0, 1, 2, 3, 4, 5, 6, 7, 8, 9, 0, 1, 2, 3, 4, 5, 6, 7, 8, 9])),
	};
}

function setup() {
	const tx = new TransactionBlock();
	tx.setSender('0x2');
	tx.setGasPrice(5);
	tx.setGasBudget(100);
	tx.setGasPayment([ref()]);
	return tx;
}
