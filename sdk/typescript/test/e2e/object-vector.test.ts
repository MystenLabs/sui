// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import { beforeEach, describe, expect, it } from 'vitest';

import { Transaction } from '../../src/transactions';
import { SUI_FRAMEWORK_ADDRESS } from '../../src/utils';
import { publishPackage, setup, TestToolbox } from './utils/setup';

describe('Test Move call with a vector of objects as input', () => {
	let toolbox: TestToolbox;
	let packageId: string;

	async function mintObject(val: number) {
		const tx = new Transaction();
		tx.moveCall({
			target: `${packageId}::entry_point_vector::mint`,
			arguments: [tx.pure.u64(val)],
		});
		const result = await toolbox.client.signAndExecuteTransaction({
			signer: toolbox.keypair,
			transaction: tx,
			options: {
				showEffects: true,
			},
		});

		await toolbox.client.waitForTransaction({ digest: result.digest });
		expect(result.effects?.status.status).toEqual('success');
		return result.effects?.created![0].reference.objectId!;
	}

	async function destroyObjects(objects: string[], withType = false) {
		const tx = new Transaction();
		const vec = tx.makeMoveVec({
			elements: objects.map((id) => tx.object(id)),
			type: withType ? `${packageId}::entry_point_vector::Obj` : undefined,
		});
		tx.moveCall({
			target: `${packageId}::entry_point_vector::two_obj_vec_destroy`,
			arguments: [vec],
		});
		const result = await toolbox.client.signAndExecuteTransaction({
			signer: toolbox.keypair,
			transaction: tx,
			options: {
				showEffects: true,
			},
		});
		await toolbox.client.waitForTransaction({ digest: result.digest });
		expect(result.effects?.status.status).toEqual('success');
	}

	beforeEach(async () => {
		toolbox = await setup();
		const packagePath =
			__dirname + '/../../../../crates/sui-core/src/unit_tests/data/entry_point_vector';
		({ packageId } = await publishPackage(packagePath));
	});

	it('Test object vector', async () => {
		await destroyObjects([(await mintObject(7))!, await mintObject(42)], /* withType */ false);
	});

	it(
		'Test object vector with type hint',
		async () => {
			await destroyObjects([await mintObject(7), await mintObject(42)], /* withType */ true);
		},
		{
			// TODO: This test is currently flaky, so adding a retry to unblock merging
			retry: 10,
		},
	);

	it('Test regular arg mixed with object vector arg', async () => {
		const coins = await toolbox.getGasObjectsOwnedByAddress();
		const coin = coins.data[3];
		const coinIDs = coins.data.map((coin) => coin.coinObjectId);
		const tx = new Transaction();
		const vec = tx.makeMoveVec({
			elements: [coinIDs[1], tx.object(coinIDs[2])],
		});
		tx.moveCall({
			target: `${SUI_FRAMEWORK_ADDRESS}::pay::join_vec`,
			typeArguments: ['0x2::sui::SUI'],
			arguments: [tx.object(coinIDs[0]), vec],
		});
		tx.setGasPayment([{ objectId: coin.coinObjectId, digest: coin.digest, version: coin.version }]);
		const result = await toolbox.client.signAndExecuteTransaction({
			signer: toolbox.keypair,
			transaction: tx,
			options: {
				showEffects: true,
			},
		});
		expect(result.effects?.status.status).toEqual('success');
	});
});
