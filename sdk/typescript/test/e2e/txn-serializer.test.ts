// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import { beforeAll, describe, expect, it } from 'vitest';

import { SharedObjectRef } from '../../src/bcs';
import { SuiTransactionBlockResponse } from '../../src/client';
import { BuilderCallArg, TransactionBlock } from '../../src/transactions';
import { TransactionBlockDataBuilder } from '../../src/transactions/TransactionBlockData';
import { SUI_SYSTEM_STATE_OBJECT_ID } from '../../src/utils';
import { publishPackage, setup, TestToolbox } from './utils/setup';

describe('Transaction Serialization and deserialization', () => {
	let toolbox: TestToolbox;
	let packageId: string;
	let publishTxn: SuiTransactionBlockResponse;
	let sharedObjectId: string;

	beforeAll(async () => {
		toolbox = await setup();
		const packagePath = __dirname + '/./data/serializer';
		({ packageId, publishTxn } = await publishPackage(packagePath));
		const sharedObject = (publishTxn.effects?.created)!.filter(
			(o) =>
				typeof o.owner === 'object' &&
				'Shared' in o.owner &&
				o.owner.Shared.initial_shared_version !== undefined,
		)[0];
		sharedObjectId = sharedObject.reference.objectId;
	});

	async function serializeAndDeserialize(tx: TransactionBlock, mutable: boolean[]) {
		tx.setSender(await toolbox.address());
		const transactionBlockBytes = await tx.build({
			client: toolbox.client,
		});
		const deserializedTxnBuilder = TransactionBlockDataBuilder.fromBytes(transactionBlockBytes);
		expect(
			deserializedTxnBuilder.inputs
				.filter((i) => isSharedObjectInput(i.value))
				.map((i) => isMutableSharedObjectInput(i.value)),
		).toStrictEqual(mutable);
		const reserializedTxnBytes = await deserializedTxnBuilder.build();
		expect(reserializedTxnBytes).toEqual(transactionBlockBytes);
	}

	// TODO: Re-enable when this isn't broken
	it.skip('Move Shared Object Call with mutable reference', async () => {
		const coins = await toolbox.getGasObjectsOwnedByAddress();

		const [{ suiAddress: validatorAddress }] = await toolbox.getActiveValidators();

		const tx = new TransactionBlock();
		const coin = coins.data[2];
		tx.moveCall({
			target: '0x3::sui_system::request_add_stake',
			arguments: [
				tx.object(SUI_SYSTEM_STATE_OBJECT_ID),
				tx.object(coin.coinObjectId),
				tx.pure(validatorAddress),
			],
		});
		await serializeAndDeserialize(tx, [true]);
	});

	it('Move Shared Object Call with immutable reference', async () => {
		const tx = new TransactionBlock();
		tx.moveCall({
			target: `${packageId}::serializer_tests::value`,
			arguments: [tx.object(sharedObjectId)],
		});
		await serializeAndDeserialize(tx, [false]);
	});

	it('Move Shared Object Call with mixed usage of mutable and immutable references', async () => {
		const tx = new TransactionBlock();
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
		const tx = new TransactionBlock();
		tx.setExpiration({ Epoch: 100 });
		await serializeAndDeserialize(tx, []);
	});
});

export function getSharedObjectInput(arg: BuilderCallArg): SharedObjectRef | undefined {
	return typeof arg === 'object' && 'Object' in arg && 'Shared' in arg.Object
		? arg.Object.Shared
		: undefined;
}

export function isSharedObjectInput(arg: BuilderCallArg): boolean {
	return !!getSharedObjectInput(arg);
}

export function isMutableSharedObjectInput(arg: BuilderCallArg): boolean {
	return getSharedObjectInput(arg)?.mutable ?? false;
}
