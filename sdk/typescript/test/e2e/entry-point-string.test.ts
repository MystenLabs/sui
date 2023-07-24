// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import { describe, it, expect, beforeAll } from 'vitest';
import { getExecutionStatusType } from '../../src';
import { TransactionBlock } from '../../src/builder';
import { publishPackage, setup, TestToolbox } from './utils/setup';

describe('Test Move call with strings', () => {
	let toolbox: TestToolbox;
	let packageId: string;

	async function callWithString(str: string | string[], len: number, funcName: string) {
		const tx = new TransactionBlock();
		tx.moveCall({
			target: `${packageId}::entry_point_types::${funcName}`,
			arguments: [tx.pure(str), tx.pure(len)],
		});
		const result = await toolbox.client.signAndExecuteTransactionBlock({
			transactionBlock: tx,
			signer: toolbox.keypair,
			options: {
				showEffects: true,
			},
		});
		expect(getExecutionStatusType(result)).toEqual('success');
	}

	beforeAll(async () => {
		toolbox = await setup();
		const packagePath =
			__dirname + '/../../../../crates/sui-core/src/unit_tests/data/entry_point_types';
		({ packageId } = await publishPackage(packagePath));
	});

	it('Test ascii', async () => {
		const s = 'SomeString';
		await callWithString(s, s.length, 'ascii_arg');
	});

	it('Test utf8', async () => {
		const s = 'çå∞≠¢õß∂ƒ∫';
		const byte_len = 24;
		await callWithString(s, byte_len, 'utf8_arg');
	});

	it('Test string vec', async () => {
		const s1 = 'çå∞≠¢';
		const s2 = 'õß∂ƒ∫';
		const byte_len = 24;
		await callWithString([s1, s2], byte_len, 'utf8_vec_arg');
	});
});
