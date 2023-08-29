// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import { type TransactionBlock } from '@mysten/sui.js/transactions';
import {
	attachFloorPriceRuleTx,
	attachKioskLockRuleTx,
	attachPersonalKioskRuleTx,
	attachRoyaltyRuleTx,
} from '../tx/rules/attach';
import { Network, KioskClientOptions } from './kiosk-client';
import {
	createTransferPolicy,
	removeTransferPolicyRule,
	withdrawFromPolicy,
} from '../tx/transfer-policy';
import { ObjectArgument, TRANSFER_POLICY_CAP_TYPE } from '../types';
import { queryTransferPolicy } from '../query/transfer-policy';
import { SuiClient } from '@mysten/sui.js/src/client';
import {
	floorPriceRuleAddress,
	kioskLockRuleAddress,
	personalKioskAddress,
	royaltyRuleAddress,
} from '../constants';

export class TransferPolicyClient {
	client: SuiClient;
	network: Network;
	policyId?: string;
	policyCap?: string;
	itemType?: string;

	constructor(options: KioskClientOptions) {
		this.client = options.client;
		this.network = options.network;
	}

	/**
	 * A function to create a new transfer policy.
	 * Checks if there's already an existing transfer policy to prevent
	 * double transfer polciy mistakes.
	 * There's an optional `skipCheck` flag that will just create the policy
	 * without checking
	 *
	 * @param tx The Transaction Block
	 * @param itemType The Type (`T`) for which we're creating the transfer policy.
	 * @param publisher The Publisher Object Id.
	 * @param address Address to save the `TransferPolicyCap` object to.
	 */
	async create(
		tx: TransactionBlock,
		itemType: string,
		publisher: ObjectArgument,
		address: string,
		options?: {
			skipCheck?: boolean;
		},
	) {
		if (!options || !options.skipCheck) {
			let policies = await queryTransferPolicy(this.client, itemType);
			if (policies.length > 0) throw new Error("There's already transfer policy for this Type.");
		}
		const cap = createTransferPolicy(tx, itemType, publisher);
		tx.transferObjects([cap], tx.pure(address, 'address'));
	}

	/**
	 * Setup the TransferPolicy object by passing just the policyCapId.
	 * It automatically finds the policyId, as well as it's type.
	 * @param policyCapId The Object ID for the TransferPolicyCap object
	 */
	async setPolicyAsync(policyCapId: string) {
		let capObject = await this.client.getObject({
			id: policyCapId,
			options: {
				showContent: true,
			},
		});
		if (!capObject) throw new Error("This cap Object wasn't found");

		let type = (capObject?.data?.content as { type: string })?.type;
		//@ts-ignore-next-line
		let policyId = capObject?.data?.content?.fields?.policy_id;

		if (!type.includes(TRANSFER_POLICY_CAP_TYPE))
			throw new Error('Invalid Cap Object Id. Are you sure this ID is a cap?');

		// Transform 0x2::transfer_policy::TransferPolicyCap<itemType> -> itemType
		let itemType = type.replace(TRANSFER_POLICY_CAP_TYPE + '<', '').slice(0, -1);

		this.setPolicy(policyId, policyCapId, itemType);
	}

	/**
	 * Set Policy by passing the types / ids manually.
	 * Use `setPolicyAsync` to automatically fetch them by just passing the Cap's object Id.
	 * @param policyId The `TransferPolicy` Object ID
	 * @param policyCap The `TransferPolicyCap` Object ID
	 * @param type The `T` (type) for the `TransferPolicy`
	 */
	setPolicy(policyId: string, policyCap: string, type: string) {
		this.setPolicyId(policyId);
		this.setPolicyCap(policyCap);
		this.setPolicyType(type);
	}

	setPolicyId(id: string) {
		this.policyId = id;
	}

	setPolicyCap(capId: string) {
		this.policyCap = capId;
	}

	setPolicyType(type: string) {
		this.itemType = type;
	}

	/**
	 * Withdraw from the transfer policy's profits.
	 * @param tx The Transaction Block.
	 * @param address Address to transfer the profits to.
	 * @param amount Optional amount parameter. Will withdraw all profits if the amount is not specified.
	 */
	withdraw(tx: TransactionBlock, address: string, amount?: string | bigint) {
		this.#validateInputs();
		// Withdraw coin for specified amount (or none)
		let coin = withdrawFromPolicy(tx, this.itemType!, this.policyId!, this.policyCap!, amount);

		tx.transferObjects([coin], tx.pure(address, 'address'));
	}

	/**
	 *  Adds the Kiosk Royalty rule to the Transfer Policy.
	 *  You can pass the percentage, as well as a minimum amount.
	 *  The royalty that will be paid is the MAX(percentage, minAmount).
	 * 	You can pass 0 in either value if you want only percentage royalty, or a fixed amount fee.
	 * 	(but you should define at least one of them for the rule to make sense).
	 *
	 * @param tx The Transaction Block
	 * 	@param percentageBps The royalty percentage in basis points. Use `percentageToBasisPoints` helper to convert from percentage [0,100].
	 * 	@param minAmount The minimum royalty amount per request in MIST.
	 */
	addRoyaltyRule(
		tx: TransactionBlock,
		percentageBps: number | string, // this is in basis points.
		minAmount: number | string,
	) {
		this.#validateInputs();

		// Hard-coding package Ids as these don't change.
		// Also, it's hard to keep versioning as with network wipes, mainnet
		// and testnet will conflict.
		attachRoyaltyRuleTx(
			tx,
			this.itemType!,
			this.policyId!,
			this.policyCap!,
			percentageBps,
			minAmount,
			royaltyRuleAddress[this.network],
		);
	}

	/**
	 * Adds the Kiosk Lock Rule to the Transfer Policy.
	 * This Rule forces buyer to lock the item in the kiosk, preserving strong royalties.
	 *
	 * @param tx The Transaction Block
	 */
	addLockRule(tx: TransactionBlock) {
		this.#validateInputs();

		attachKioskLockRuleTx(
			tx,
			this.itemType!,
			this.policyId!,
			this.policyCap!,
			kioskLockRuleAddress[this.network],
		);
	}

	/**
	 * Attaches the Personal Kiosk Rule, making a purchase valid only for `SoulBound` kiosks.
	 * @param tx The Transaction Block
	 */
	addPersonalKioskRule(tx: TransactionBlock) {
		this.#validateInputs();

		attachPersonalKioskRuleTx(
			tx,
			this.itemType!,
			this.policyId!,
			this.policyCap!,
			personalKioskAddress[this.network],
		);
	}

	/**
	 * A function to add the floor price rule to a transfer policy.
	 * @param minPrice The minimum price in MIST.
	 */
	addFloorPriceRule(tx: TransactionBlock, minPrice: string | number) {
		this.#validateInputs();

		attachFloorPriceRuleTx(
			tx,
			this.itemType!,
			this.policyId!,
			this.policyCap!,
			minPrice,
			floorPriceRuleAddress[this.network],
		);
	}

	removeLockRule(tx: TransactionBlock) {
		this.#validateInputs();

		let packageId = kioskLockRuleAddress[this.network];

		removeTransferPolicyRule(
			tx,
			this.itemType!,
			`${packageId}::kiosk_lock_rule::Rule`,
			`${packageId}::kiosk_lock_rule::Config`,
			this.policyId!,
			this.policyCap!,
		);
	}

	removeRoyaltyRule(tx: TransactionBlock) {
		this.#validateInputs();

		let packageId = royaltyRuleAddress[this.network];

		removeTransferPolicyRule(
			tx,
			this.itemType!,
			`${packageId}::royalty_rule::Rule`,
			`${packageId}::royalty_rule::Config`,
			this.policyId!,
			this.policyCap!,
		);
	}

	removePersonalKioskRule(tx: TransactionBlock) {
		this.#validateInputs();

		let packageId = personalKioskAddress[this.network];

		removeTransferPolicyRule(
			tx,
			this.itemType!,
			`${packageId}::personal_kiosk_rule::Rule`,
			`bool`,
			this.policyId!,
			this.policyCap!,
		);
	}

	removeFloorPriceRule(tx: TransactionBlock) {
		this.#validateInputs();

		let packageId = floorPriceRuleAddress[this.network];

		removeTransferPolicyRule(
			tx,
			this.itemType!,
			`${packageId}::floor_price_rule::Rule`,
			`${packageId}::floor_price_rule::Config`,
			this.policyId!,
			this.policyCap!,
		);
	}

	// Internal function that that the policy's Id + Cap + type have been set.
	#validateInputs() {
		if (!this.policyId) throw new Error('Please set the Transfer Policy Id.');
		if (!this.policyCap) throw new Error('Please set the Transfer Policy Cap Id');
		if (!this.itemType)
			throw new Error('Please set the Transfer Policy Item Type (e.g. `{packageId}::item::Item`)');
	}
}
