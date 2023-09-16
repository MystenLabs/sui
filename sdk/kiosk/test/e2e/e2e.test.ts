// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import { describe, it, beforeAll, expect } from 'vitest';

import {
	TestToolbox,
	setupSuiClient,
	publishExtensionsPackage,
	publishHeroPackage,
	mintHero,
	createKiosk,
	createPersonalKiosk,
	executeTransactionBlock,
	mintVillain,
} from './setup';
import {
	KioskClient,
	KioskTransaction,
	Network,
	TransferPolicyTransaction,
	percentageToBasisPoints,
} from '../../src';
import {
	existingKioskManagementFlow,
	prepareHeroRuleset,
	prepareVillainTransferPolicy,
	purchaseFlow,
	purchaseOnNewKiosk,
	testLockItemFlow,
} from './helper';
import { TransactionBlock } from '@mysten/sui.js/transactions';

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

		const txb = new TransactionBlock();
		const kioskTx = new KioskTransaction({ kioskClient, txb, cap: kioskOwnerCaps[0] });

		kioskTx.place({
			itemType: heroType,
			item: heroId,
		});
		const [item, promise] = kioskTx.borrow({
			itemType: heroType,
			itemId: heroId,
		});

		txb.moveCall({
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
				txb.moveCall({
					target: `${heroPackageId}::hero::level_up`,
					arguments: [item],
				});
			},
		);

		kioskTx.wrap();
		await executeTransactionBlock(toolbox, txb);
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

		const txb = new TransactionBlock();
		const tpTx = new TransferPolicyTransaction({ kioskClient, txb, cap: villainPolicyCaps[0] });

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

		await executeTransactionBlock(toolbox, txb);
	});
});
