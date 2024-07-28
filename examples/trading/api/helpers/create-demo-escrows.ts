// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import { Transaction } from '@mysten/sui/transactions';

import { CONFIG } from '../config';
import { getActiveAddress, getClient, signAndExecute } from '../sui-utils';

/// A sample on how we could fetch our owned bears that we created.
/// We're formatting them in an easy to use way for next steps of our demo.
const getOwnedBears = async () => {
	const client = getClient(CONFIG.NETWORK);

	const res = await client.getOwnedObjects({
		filter: {
			StructType: `${CONFIG.DEMO_CONTRACT.packageId}::demo_bear::DemoBear`,
		},
		options: {
			showContent: true,
			showType: true,
		},
		owner: getActiveAddress(),
	});

	const formatted = res.data.map((x) => {
		return {
			objectId: x.data?.objectId,
			type: x.data?.type,
		};
	});

	return formatted;
};

/// A demo to fetch our owned locked objects, used in our `createEscrows` demo function.
const getOwnedLockedItems = async () => {
	const client = getClient(CONFIG.NETWORK);

	const res = await client.getOwnedObjects({
		filter: {
			StructType: `${CONFIG.SWAP_CONTRACT.packageId}::lock::Locked`,
		},
		options: {
			showContent: true,
			showType: true,
		},
		owner: getActiveAddress(),
	});

	const formatted = res.data.map((x) => {
		return {
			objectId: x.data?.objectId,
			key: (x.data?.content as any)?.fields?.key,
			type: x.data?.type,
		};
	});

	return formatted;
};

/// Uses the data created on `create-demo-data.ts` to create escrows between them.
const createEscrows = async (total: number) => {
	const lockedObjects = await getOwnedLockedItems();
	const ownedBears = await getOwnedBears();

	if (!lockedObjects || !ownedBears || lockedObjects.length < total || ownedBears.length < total)
		throw new Error(
			`please run 'ts-node create-demo-data.ts' first, with at least ${total} bears.`,
		);

	// Split the array into chunks of 2, so we can create escrows between them.
	const tuples = [];
	for (let i = 0; i < total; i += 1) {
		tuples.push({
			bear: ownedBears[i],
			locked: lockedObjects[i],
		});
	}

	const txb = new Transaction();

	for (const tuple of tuples) {
		if (!tuple.bear) break;
		if (!tuple.bear.objectId) throw new Error('bear.objectId is not defined. Does not make sense!');

		txb.moveCall({
			target: `${CONFIG.SWAP_CONTRACT.packageId}::shared::create`,
			arguments: [
				txb.object(tuple.bear.objectId),
				txb.pure.address(tuple.locked.key),
				txb.pure.address(getActiveAddress()),
			],
			typeArguments: [tuple.bear.type!],
		});
	}

	const res = await signAndExecute(txb, CONFIG.NETWORK);
	if (!res.objectChanges || res.objectChanges.length === 0)
		throw new Error('Something went wrong while creating escrows.');

	console.log('Successfully created escrows.');
};

createEscrows(10);
