// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import { TransactionArgument, type TransactionBlock } from '@mysten/sui.js/transactions';
import {
	attachFloorPriceRuleTx,
	attachKioskLockRuleTx,
	attachPersonalKioskRuleTx,
	attachRoyaltyRuleTx,
} from '../tx/rules/attach';
import {
	createTransferPolicy,
	createTransferPolicyWithoutSharing,
	removeTransferPolicyRule,
	shareTransferPolicy,
	withdrawFromPolicy,
} from '../tx/transfer-policy';
import {
	type KioskClientOptions,
	type Network,
	type ObjectArgument,
	TRANSFER_POLICY_CAP_TYPE,
} from '../types';
import { queryOwnedTransferPolicyCap, queryTransferPolicy } from '../query/transfer-policy';
import { SuiClient } from '@mysten/sui.js/src/client';
import {
	FLOOR_PRICE_RULE_ADDRESS,
	KIOSK_LOCK_RULE_ADDRESS,
	PERSONAL_KIOSK_RULE_ADDRESS,
	ROYALTY_RULE_ADDRESS,
} from '../constants';

export type TransferPolicyBaseParams = {
	type: string;
	publisher: ObjectArgument;
	skipCheck?: boolean;
};

export class TransferPolicyManager {
	client: SuiClient;
	network: Network;
	policyId?: ObjectArgument;
	policyCap?: ObjectArgument;
	type?: string;

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
	 * @param type The Type (`T`) for which we're creating the transfer policy.
	 * @param publisher The Publisher Object Id.
	 * @param address Address to save the `TransferPolicyCap` object to.
	 * @param skipCheck (Optional) skip checking if a transfer policy already exists
	 */
	async createAndShare(
		txb: TransactionBlock,
		{
			type,
			publisher,
			address,
			skipCheck,
		}: TransferPolicyBaseParams & {
			address: string;
		},
	) {
		if (!skipCheck) {
			const policies = await queryTransferPolicy(this.client, type);
			if (policies.length > 0) throw new Error("There's already transfer policy for this Type.");
		}
		const cap = createTransferPolicy(txb, type, publisher);
		txb.transferObjects([cap], txb.pure(address, 'address'));
	}

	/**
	 * A convenient function to create a Transfer Policy and attach some rules
	 * before sharing it (so you can prepare it in a single PTB)
	 * @param tx The Transaction Block
	 * @param type The Type (`T`) for which we're creating the transfer policy.
	 * @param publisher The Publisher Object Id.
	 * @param address Address to save the `TransferPolicyCap` object to.
	 * @param skipCheck (Optional) skip checking if a transfer policy already exists
	 */
	async create(
		txb: TransactionBlock,
		{ type, publisher, skipCheck }: TransferPolicyBaseParams,
	): Promise<TransferPolicyManager> {
		if (!skipCheck) {
			const policies = await queryTransferPolicy(this.client, type);
			if (policies.length > 0) throw new Error("There's already transfer policy for this Type.");
		}
		const [policy, policyCap] = createTransferPolicyWithoutSharing(txb, type, publisher);

		this.setPolicy(policy, policyCap, type); // sets the client's TP to the newly created one.
		return this;
	}

	/**
	 * This can be called after calling the `create` function to share the `TransferPolicy`,
	 * and transfer the `TransferPolicyCap` to the specified address
	 *
	 * @param txb The Transaction Block
	 * @param address The address to transfer the `TransferPolicyCap`
	 */
	shareAndTransferCap(txb: TransactionBlock, address: string) {
		if (!this.type || !this.policyCap || !this.policyId)
			throw new Error('This function can only be called after `transferPolicyManager.create`');

		shareTransferPolicy(txb, this.type, this.policyId as TransactionArgument);
		txb.transferObjects([this.policyCap as TransactionArgument], txb.pure(address, 'address'));

		this.#resetPolicy();
	}

	#resetPolicy() {
		this.policyCap = undefined;
		this.policyId = undefined;
	}

	/**
	 * Find the Policy Cap Object for a specified address.
	 * Returns null if the address doesn't own it.
	 * @param type The Type of the object
	 * @param address The address that owns the type
	 */
	async getPolicyCapId(type: string, address: string) {
		return queryOwnedTransferPolicyCap(this.client, address, type);
	}

	/**
	 * Setup the TransferPolicy object by passing the itemType and the owner address.
	 * @param type The Type for which we're managing the transfer policy
	 * @param address The owner of the Cap.
	 */
	async setPolicyByTypeAsync(type: string, address: string) {
		const policyCapId = await this.getPolicyCapId(type, address);
		if (!policyCapId)
			throw new Error(
				`Couldn't find a TransferPolicyCap for type ${type} owned by address ${address}`,
			);

		return this.setPolicyAsync(policyCapId);
	}

	/**
	 * Setup the TransferPolicy object by passing just the policyCapId.
	 * It automatically finds the policyId, as well as it's type.
	 * @param policyCapId The Object ID for the TransferPolicyCap object
	 */
	async setPolicyAsync(policyCapId: string) {
		const capObject = await this.client.getObject({
			id: policyCapId,
			options: {
				showContent: true,
			},
		});
		if (!capObject) throw new Error("This cap Object wasn't found");

		const type = (capObject?.data?.content as { type: string })?.type;
		//@ts-ignore-next-line
		const policy = capObject?.data?.content?.fields?.policy_id;

		if (!type.includes(TRANSFER_POLICY_CAP_TYPE))
			throw new Error('Invalid Cap Object Id. Are you sure this ID is a cap?');

		// Transform 0x2::transfer_policy::TransferPolicyCap<itemType> -> itemType
		const objectType = type.replace(TRANSFER_POLICY_CAP_TYPE + '<', '').slice(0, -1);

		this.setPolicy(policy, policyCapId, objectType);
	}

	/**
	 * Set Policy by passing the types / ids manually.
	 * Use `setPolicyAsync` to automatically fetch them by just passing the Cap's object Id.
	 * @param policy The `TransferPolicy` Object ID (or object ref)
	 * @param policyCap The `TransferPolicyCap` Object ID (or object ref)
	 * @param type The `T` (type) for the `TransferPolicy`
	 */
	setPolicy(policyId: ObjectArgument, policyCap: ObjectArgument, type: string) {
		this.setPolicyId(policyId).setPolicyCap(policyCap).setPolicyType(type);
		return this;
	}

	setPolicyId(policyId: ObjectArgument) {
		this.policyId = policyId;
		return this;
	}

	setPolicyCap(policyCap: ObjectArgument) {
		this.policyCap = policyCap;
		return this;
	}

	setPolicyType(type: string) {
		this.type = type;
		return this;
	}

	/**
	 * Withdraw from the transfer policy's profits.
	 * @param tx The Transaction Block.
	 * @param address Address to transfer the profits to.
	 * @param amount Optional amount parameter. Will withdraw all profits if the amount is not specified.
	 */
	withdraw(txb: TransactionBlock, address: string, amount?: string | bigint) {
		this.#validateInputs();
		// Withdraw coin for specified amount (or none)
		const coin = withdrawFromPolicy(txb, this.type!, this.policyId!, this.policyCap!, amount);

		txb.transferObjects([coin], txb.pure(address, 'address'));

		return this;
	}

	/**
	 *  Adds the Kiosk Royalty rule to the Transfer Policy.
	 *  You can pass the percentage, as well as a minimum amount.
	 *  The royalty that will be paid is the MAX(percentage, minAmount).
	 * 	You can pass 0 in either value if you want only percentage royalty, or a fixed amount fee.
	 * 	(but you should define at least one of them for the rule to make sense).
	 *
	 * 	@param tx The Transaction Block
	 * 	@param percentageBps The royalty percentage in basis points. Use `percentageToBasisPoints` helper to convert from percentage [0,100].
	 * 	@param minAmount The minimum royalty amount per request in MIST.
	 */
	addRoyaltyRule(
		txb: TransactionBlock,
		percentageBps: number | string, // this is in basis points.
		minAmount: number | string,
	) {
		this.#validateInputs();

		// Hard-coding package Ids as these don't change.
		// Also, it's hard to keep versioning as with network wipes, mainnet
		// and testnet will conflict.
		attachRoyaltyRuleTx(
			txb,
			this.type!,
			this.policyId!,
			this.policyCap!,
			percentageBps,
			minAmount,
			ROYALTY_RULE_ADDRESS[this.network],
		);
		return this;
	}

	/**
	 * Adds the Kiosk Lock Rule to the Transfer Policy.
	 * This Rule forces buyer to lock the item in the kiosk, preserving strong royalties.
	 *
	 * @param tx The Transaction Block
	 */
	addLockRule(txb: TransactionBlock) {
		this.#validateInputs();

		attachKioskLockRuleTx(
			txb,
			this.type!,
			this.policyId!,
			this.policyCap!,
			KIOSK_LOCK_RULE_ADDRESS[this.network],
		);
		return this;
	}

	/**
	 * Attaches the Personal Kiosk Rule, making a purchase valid only for `SoulBound` kiosks.
	 * @param txb The Transaction Block
	 */
	addPersonalKioskRule(txb: TransactionBlock) {
		this.#validateInputs();

		attachPersonalKioskRuleTx(
			txb,
			this.type!,
			this.policyId!,
			this.policyCap!,
			PERSONAL_KIOSK_RULE_ADDRESS[this.network],
		);
		return this;
	}

	/**
	 * A function to add the floor price rule to a transfer policy.
	 * @param txb The Transaction Block
	 * @param minPrice The minimum price in MIST.
	 */
	addFloorPriceRule(txb: TransactionBlock, minPrice: string | bigint) {
		this.#validateInputs();

		attachFloorPriceRuleTx(
			txb,
			this.type!,
			this.policyId!,
			this.policyCap!,
			minPrice,
			FLOOR_PRICE_RULE_ADDRESS[this.network],
		);
		return this;
	}

	/**
	 * Generic helper to remove a rule, not from the SDK's base ruleset.
	 * @param txb The Transaction Block
	 * @param ruleType The Rule Type
	 * @param configType The Config Type
	 */
	removeRule({
		txb,
		ruleType,
		configType,
	}: {
		txb: TransactionBlock;
		ruleType: string;
		configType: string;
	}) {
		this.#validateInputs();

		removeTransferPolicyRule(
			txb,
			this.type!,
			ruleType,
			configType,
			this.policyId!,
			this.policyCap!,
		);
	}

	/**
	 * Removes the lock rule
	 * @param txb The Transaction Block
	 */
	removeLockRule(txb: TransactionBlock) {
		this.#validateInputs();

		const packageId = KIOSK_LOCK_RULE_ADDRESS[this.network];

		removeTransferPolicyRule(
			txb,
			this.type!,
			`${packageId}::kiosk_lock_rule::Rule`,
			`${packageId}::kiosk_lock_rule::Config`,
			this.policyId!,
			this.policyCap!,
		);
		return this;
	}

	/**
	 * Removes the Royalty rule
	 * @param txb The Transaction Block
	 */
	removeRoyaltyRule(txb: TransactionBlock) {
		this.#validateInputs();

		const packageId = ROYALTY_RULE_ADDRESS[this.network];

		removeTransferPolicyRule(
			txb,
			this.type!,
			`${packageId}::royalty_rule::Rule`,
			`${packageId}::royalty_rule::Config`,
			this.policyId!,
			this.policyCap!,
		);
		return this;
	}

	removePersonalKioskRule(txb: TransactionBlock) {
		this.#validateInputs();

		const packageId = PERSONAL_KIOSK_RULE_ADDRESS[this.network];

		removeTransferPolicyRule(
			txb,
			this.type!,
			`${packageId}::personal_kiosk_rule::Rule`,
			`bool`,
			this.policyId!,
			this.policyCap!,
		);
		return this;
	}

	removeFloorPriceRule(txb: TransactionBlock) {
		this.#validateInputs();

		const packageId = FLOOR_PRICE_RULE_ADDRESS[this.network];

		removeTransferPolicyRule(
			txb,
			this.type!,
			`${packageId}::floor_price_rule::Rule`,
			`${packageId}::floor_price_rule::Config`,
			this.policyId!,
			this.policyCap!,
		);
		return this;
	}

	// Internal function that that the policy's Id + Cap + type have been set.
	#validateInputs() {
		const genericErrorMessage = `Please use 'setPolicyAsync' or 'setPolicy' to setup the TransferPolicy.`;
		if (!this.policyId)
			throw new Error(`${genericErrorMessage} Missing: Transfer Policy Object ID.`);
		if (!this.policyCap)
			throw new Error(`${genericErrorMessage} Missing: TransferPolicyCap Object ID`);
		if (!this.type)
			throw new Error(
				`${genericErrorMessage} Missing: Transfer Policy Item Type (e.g. {packageId}::item::Item)`,
			);
	}
}
