// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import { Transaction } from '@mysten/sui/transactions';
import { expect } from 'vitest';

import type { KioskClient, KioskOwnerCap } from '../../src/index.js';
import {
	KioskTransaction,
	percentageToBasisPoints,
	TransferPolicyTransaction,
} from '../../src/index.js';
import type { TestToolbox } from './setup.js';
import { executeTransaction, getPublisherObject } from './setup.js';

// Creates a fresh transfer policy for Heroes and attaches all the rules.
export async function prepareHeroRuleset({
	toolbox,
	heroPackageId,
	kioskClient,
}: {
	toolbox: TestToolbox;
	heroPackageId: string;
	kioskClient: KioskClient;
}) {
	/// Do a full rule setup for `Hero` type.
	const publisher = await getPublisherObject(toolbox);
	const tx = new Transaction();
	const tpTx = new TransferPolicyTransaction({ kioskClient, transaction: tx });

	await tpTx.create({
		type: `${heroPackageId}::hero::Hero`,
		publisher,
	});

	tpTx
		.addLockRule()
		.addFloorPriceRule(1000n)
		.addRoyaltyRule(percentageToBasisPoints(10), 100)
		.addPersonalKioskRule()
		.shareAndTransferCap(toolbox.address());

	await executeTransaction(toolbox, tx);
}

// Creates a fresh transfer policy for Heroes and attaches all the rules.
export async function prepareVillainTransferPolicy({
	toolbox,
	heroPackageId,
	kioskClient,
}: {
	toolbox: TestToolbox;
	heroPackageId: string;
	kioskClient: KioskClient;
}) {
	/// Do a plain TP creation for `Villain` type.
	const publisher = await getPublisherObject(toolbox);
	const tx = new Transaction();
	const tpTx = new TransferPolicyTransaction({ kioskClient, transaction: tx });

	await tpTx.createAndShare({
		type: `${heroPackageId}::hero::Villain`,
		publisher,
		address: toolbox.address(),
	});

	await executeTransaction(toolbox, tx);
}

export async function testLockItemFlow(
	toolbox: TestToolbox,
	kioskClient: KioskClient,
	cap: KioskOwnerCap,
	itemType: string,
	itemId: string,
) {
	const tx = new Transaction();
	const kioskTx = new KioskTransaction({ transaction: tx, kioskClient, cap });

	const policies = await kioskClient.getTransferPolicies({ type: itemType });
	expect(policies).toHaveLength(1);

	kioskTx
		.lock({
			itemType,
			item: itemId,
			policy: policies[0].id,
		})
		.finalize();

	await executeTransaction(toolbox, tx);
}

// A helper that does a full run for kiosk management.
export async function existingKioskManagementFlow(
	toolbox: TestToolbox,
	kioskClient: KioskClient,
	cap: KioskOwnerCap,
	itemType: string,
	itemId: string,
) {
	const tx = new Transaction();
	const kioskTx = new KioskTransaction({ transaction: tx, kioskClient, cap });

	kioskTx
		.place({
			itemType,
			item: itemId,
		})
		.list({
			itemType,
			itemId: itemId,
			price: 100000n,
		})
		.delist({
			itemType,
			itemId: itemId,
		});

	const item = kioskTx.take({
		itemType,
		itemId: itemId,
	});

	kioskTx
		.placeAndList({
			itemType,
			item,
			price: 100000n,
		})
		.delist({
			itemType,
			itemId: itemId,
		})
		.transfer({
			itemType,
			itemId: itemId,
			address: toolbox.address(),
		})
		.withdraw(toolbox.address())
		.finalize();

	await executeTransaction(toolbox, tx);
}

/**
 * Lists an item for sale using one kiosk, and purchases it using another.
 * Depending on the rules, the buyer kiosk might have to be personal.
 */
export async function purchaseFlow(
	toolbox: TestToolbox,
	kioskClient: KioskClient,
	buyerCap: KioskOwnerCap,
	sellerCap: KioskOwnerCap,
	itemType: string,
	itemId: string,
) {
	/**
	 * Lists an item for sale
	 */
	const SALE_PRICE = 100000n;
	const sellTxb = new Transaction();
	new KioskTransaction({ transaction: sellTxb, kioskClient, cap: sellerCap })
		.placeAndList({
			itemType,
			item: itemId,
			price: SALE_PRICE,
		})
		.finalize();

	await executeTransaction(toolbox, sellTxb);

	/**
	 * Purchases the item using a different kiosk (must be personal)
	 */
	const purchaseTxb = new Transaction();
	const purchaseTx = new KioskTransaction({
		transaction: purchaseTxb,
		kioskClient,
		cap: buyerCap,
	});

	(
		await purchaseTx.purchaseAndResolve({
			itemType,
			itemId,
			sellerKiosk: sellerCap.kioskId,
			price: SALE_PRICE,
		})
	).finalize();

	await executeTransaction(toolbox, purchaseTxb);
}

export async function purchaseOnNewKiosk(
	toolbox: TestToolbox,
	kioskClient: KioskClient,
	sellerCap: KioskOwnerCap,
	itemType: string,
	itemId: string,
	personal?: boolean,
) {
	/**
	 * Lists an item for sale
	 */
	const SALE_PRICE = 100000n;
	const sellTxb = new Transaction();
	new KioskTransaction({ transaction: sellTxb, kioskClient, cap: sellerCap })
		.placeAndList({
			itemType,
			item: itemId,
			price: SALE_PRICE,
		})
		.finalize();

	await executeTransaction(toolbox, sellTxb);

	/**
	 * Purchases the item using a different kiosk (must be personal)
	 */
	const purchaseTxb = new Transaction();
	const purchaseTx = new KioskTransaction({ transaction: purchaseTxb, kioskClient });

	// create personal kiosk (`true` means that we can use this kiosk for extra transactions)
	if (personal) purchaseTx.createPersonal(true);
	else purchaseTx.create();

	// do the purchase.
	await purchaseTx.purchaseAndResolve({
		itemType,
		itemId,
		sellerKiosk: sellerCap.kioskId,
		price: SALE_PRICE,
	});
	if (!personal) purchaseTx.shareAndTransferCap(toolbox.address());
	purchaseTx.finalize();

	await executeTransaction(toolbox, purchaseTxb);
}
