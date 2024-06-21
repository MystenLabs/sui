// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import { Transaction } from '@mysten/sui/transactions';
import { normalizeSuiAddress } from '@mysten/sui/utils';
import { beforeAll, describe, expect, it } from 'vitest';

import {
	KioskClient,
	KioskTransaction,
	Network,
	percentageToBasisPoints,
	TransferPolicyTransaction,
} from '../../src';
import {
	existingKioskManagementFlow,
	prepareHeroRuleset,
	prepareVillainTransferPolicy,
	purchaseFlow,
	purchaseOnNewKiosk,
	testLockItemFlow,
} from './helper';
import {
	createKiosk,
	createPersonalKiosk,
	executeTransaction,
	mintHero,
	mintVillain,
	publishExtensionsPackage,
	publishHeroPackage,
	setupSuiClient,
	TestToolbox,
} from './setup';

/**
 * Important: We have 2 types so we can easily test transfer policy management without interference.
 * Please do not use `Villain` transfer policy for anything but testing the TP management.
 * If you wish to edit the TP, make sure to always end up having it be the same as the inital state when a case ends.
 * Alternatively, you can create a new TP for each iteration by using the TransferPolicyTransaction.
 */
describe('Testing Kiosk SDK transaction building & querying e2e', () => {
	let toolbox: TestToolbox;
	let extensionsPackageId: string;
	let heroPackageId: string;
	let kioskClient: KioskClient;
	let heroType: string;
	let villainType: string;

	beforeAll(async () => {
		toolbox = await setupSuiClient();
		extensionsPackageId = await publishExtensionsPackage(toolbox);
		heroPackageId = await publishHeroPackage(toolbox);
		heroType = `${heroPackageId}::hero::Hero`;
		villainType = `${heroPackageId}::hero::Villain`;

		kioskClient = new KioskClient({
			client: toolbox.client,
			network: Network.CUSTOM,
			packageIds: {
				kioskLockRulePackageId: extensionsPackageId,
				royaltyRulePackageId: extensionsPackageId,
				personalKioskRulePackageId: extensionsPackageId,
				floorPriceRulePackageId: extensionsPackageId,
			},
		});

		/// Prepare the hero ruleset.
		await prepareHeroRuleset({ toolbox, heroPackageId, kioskClient });
		await prepareVillainTransferPolicy({ toolbox, heroPackageId, kioskClient });
		await createKiosk(toolbox, kioskClient);
		await createPersonalKiosk(toolbox, kioskClient);
	});

	it('Should fetch the two already created owned kiosks in a single non-paginated request', async () => {
		const page = await kioskClient.getOwnedKiosks({
			address: toolbox.address(),
		});
		expect(page.hasNextPage).toBe(false);
		expect(page.kioskIds).toHaveLength(2);
		expect(page.kioskOwnerCaps).toHaveLength(2);

		const emptyPage = await kioskClient.getOwnedKiosks({
			address: toolbox.address(),
			pagination: {
				limit: 1,
				cursor: page.nextCursor!,
			},
		});
		expect(emptyPage.hasNextPage).toBe(false);
		expect(emptyPage.nextCursor).toBe(page.nextCursor);
		expect(emptyPage.kioskIds).toHaveLength(0);
		expect(emptyPage.kioskOwnerCaps).toHaveLength(0);
	});

	it('Should fetch the two already created owned kiosks in two paginated requests', async () => {
		const firstPage = await kioskClient.getOwnedKiosks({
			address: toolbox.address(),
			pagination: {
				limit: 1,
			},
		});
		expect(firstPage.hasNextPage).toBe(true);
		expect(firstPage.kioskIds).toHaveLength(1);
		expect(firstPage.kioskOwnerCaps).toHaveLength(1);

		const secondPage = await kioskClient.getOwnedKiosks({
			address: toolbox.address(),
			pagination: {
				limit: 1,
				cursor: firstPage.nextCursor!,
			},
		});
		expect(secondPage.hasNextPage).toBe(false);
		expect(secondPage.kioskIds).toHaveLength(1);
		expect(secondPage.kioskOwnerCaps).toHaveLength(1);

		const emptyPage = await kioskClient.getOwnedKiosks({
			address: toolbox.address(),
			pagination: {
				limit: 1,
				cursor: secondPage.nextCursor!,
			},
		});
		expect(emptyPage.hasNextPage).toBe(false);
		expect(emptyPage.nextCursor).toBe(secondPage.nextCursor);
		expect(emptyPage.kioskIds).toHaveLength(0);
		expect(emptyPage.kioskOwnerCaps).toHaveLength(0);
	});

	it('Should take, list, delist, place, placeAndList, transfer in a normal sequence on a normal and on a personal kiosk.', async () => {
		const heroId = await mintHero(toolbox, heroPackageId);
		const heroTwoId = await mintHero(toolbox, heroPackageId);

		const { kioskOwnerCaps } = await kioskClient.getOwnedKiosks({
			address: toolbox.address(),
		});

		expect(kioskOwnerCaps).toHaveLength(2);

		const normalKiosk = kioskOwnerCaps.find((x) => !x.isPersonal);
		const personalKiosk = kioskOwnerCaps.find((x) => x.isPersonal);

		// test non personal
		await existingKioskManagementFlow(toolbox, kioskClient, normalKiosk!, heroType, heroId);

		// test personal kiosk
		await existingKioskManagementFlow(toolbox, kioskClient, personalKiosk!, heroType, heroTwoId);
	});

	it('Should lock on a normal kiosk & a personal kiosk.', async () => {
		const heroId = await mintHero(toolbox, heroPackageId);
		const heroTwoId = await mintHero(toolbox, heroPackageId);

		const { kioskOwnerCaps } = await kioskClient.getOwnedKiosks({
			address: toolbox.address(),
		});

		await testLockItemFlow(
			toolbox,
			kioskClient,
			kioskOwnerCaps.find((x) => !x.isPersonal)!,
			heroType,
			heroId,
		);

		await testLockItemFlow(
			toolbox,
			kioskClient,
			kioskOwnerCaps.find((x) => x.isPersonal)!,
			heroType,
			heroTwoId,
		);
	});

	it('Should borrow an item, increase the level and return it to kiosk properly.', async () => {
		const heroId = await mintHero(toolbox, heroPackageId);
		const { kioskOwnerCaps } = await kioskClient.getOwnedKiosks({
			address: toolbox.address(),
		});

		const tx = new Transaction();
		const kioskTx = new KioskTransaction({
			kioskClient,
			transaction: tx,
			cap: kioskOwnerCaps[0],
		});

		kioskTx.place({
			itemType: heroType,
			item: heroId,
		});
		const [item, promise] = kioskTx.borrow({
			itemType: heroType,
			itemId: heroId,
		});

		tx.moveCall({
			target: `${heroPackageId}::hero::level_up`,
			arguments: [item],
		});

		kioskTx.return({
			itemType: heroType,
			item,
			promise,
		});

		// Let's try to increase health again by using callback style borrow
		kioskTx.borrowTx(
			{
				itemType: heroType,
				itemId: heroId,
			},
			(item) => {
				tx.moveCall({
					target: `${heroPackageId}::hero::level_up`,
					arguments: [item],
				});
			},
		);

		kioskTx.finalize();
		await executeTransaction(toolbox, tx);
	});

	it('Should purchase and resolve an item that has all rules.', async () => {
		const heroId = await mintHero(toolbox, heroPackageId);

		const { kioskOwnerCaps } = await kioskClient.getOwnedKiosks({
			address: toolbox.address(),
		});

		const personalKiosk = kioskOwnerCaps.find((x) => x.isPersonal);
		const nonPersonalKiosk = kioskOwnerCaps.find((x) => !x.isPersonal);

		await purchaseFlow(toolbox, kioskClient, personalKiosk!, nonPersonalKiosk!, heroType, heroId);
	});

	it('Should purchase in a new kiosk (& a new personal kiosk) from a personal kiosk and resolve personal kiosk rule', async () => {
		const heroId = await mintHero(toolbox, heroPackageId);
		// minting a villain who has no transfer policy rules so we can buy from a new kiosk.
		const villainId = await mintVillain(toolbox, heroPackageId);

		const { kioskOwnerCaps } = await kioskClient.getOwnedKiosks({
			address: toolbox.address(),
		});
		const personalKiosk = kioskOwnerCaps.find((x) => x.isPersonal);
		//
		await purchaseOnNewKiosk(toolbox, kioskClient, personalKiosk!, heroType, heroId, true);
		await purchaseOnNewKiosk(toolbox, kioskClient, personalKiosk!, villainType, villainId, false);
	});

	it('Should have the right amount of caps based on querying', async () => {
		const allCaps = await kioskClient.getOwnedTransferPolicies({
			address: toolbox.address(),
		});
		expect(allCaps).toHaveLength(2);

		const heroPolicyCaps = await kioskClient.getOwnedTransferPoliciesByType({
			type: heroType,
			address: toolbox.address(),
		});

		expect(heroPolicyCaps).toHaveLength(1);

		const villainPolicyCaps = await kioskClient.getOwnedTransferPoliciesByType({
			type: villainType,
			address: toolbox.address(),
		});

		expect(villainPolicyCaps).toHaveLength(1);
	});

	it('Should manage a Transfer Policy (add and remove all rules) / withdraw', async () => {
		const villainPolicyCaps = await kioskClient.getOwnedTransferPoliciesByType({
			type: villainType,
			address: toolbox.address(),
		});

		const tx = new Transaction();
		const tpTx = new TransferPolicyTransaction({
			kioskClient,
			transaction: tx,
			cap: villainPolicyCaps[0],
		});

		tpTx
			.addFloorPriceRule(10n)
			.addLockRule()
			.addRoyaltyRule(percentageToBasisPoints(10), 0)
			.addPersonalKioskRule()
			.removeFloorPriceRule()
			.removeLockRule()
			.removeRoyaltyRule()
			.removePersonalKioskRule()
			.withdraw(toolbox.address());

		await executeTransaction(toolbox, tx);
	});

	it('Should fetch a kiosk by id', async () => {
		const { kioskOwnerCaps } = await kioskClient.getOwnedKiosks({
			address: toolbox.address(),
		});

		const kiosk = await kioskClient.getKiosk({
			id: kioskOwnerCaps[0].kioskId,
			options: {
				withKioskFields: true, // this flag also returns the `kiosk` object in the response, which includes the base setup
				withListingPrices: true, // this flag returns the listing prices for listed items.
				withObjects: true, // this flag enables fetching of the objects within the kiosk (`multiGetObjects`).
				objectOptions: { showDisplay: true, showContent: true }, // works with `withObjects` flag, specifies the options of the fetching.
			},
		});

		expect(kiosk).toHaveProperty('kiosk');
		expect(normalizeSuiAddress(kiosk.kiosk?.owner || '')).toBe(
			normalizeSuiAddress(toolbox.address()),
		);
	});

	it('Should error when trying to call any function after calling finalize()', async () => {
		const kioskTx = new KioskTransaction({
			transaction: new Transaction(),
			kioskClient,
		});
		kioskTx.createPersonal().finalize();

		expect(() => kioskTx.withdraw(toolbox.address())).toThrowError(
			"You can't add more transactions to a finalized kiosk transaction.",
		);
	});
});
