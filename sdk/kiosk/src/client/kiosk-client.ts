// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import { type SuiClient } from '@mysten/sui.js/src/client';
import { fetchKiosk, getOwnedKiosks } from '../query/kiosk';
import {
	type FetchKioskOptions,
	KioskData,
	KioskOwnerCap,
	ObjectArgument,
	OwnedKiosks,
	Network,
	type KioskClientOptions,
} from '../types';
import { TransactionArgument, TransactionBlock } from '@mysten/sui.js/transactions';
import * as kioskTx from '../tx/kiosk';
import { TransferPolicyRule, personalKioskAddress, rules } from '../constants';
import { queryTransferPolicy } from '../query/transfer-policy';
import { convertToPersonalTx } from '../tx/personal-kiosk';
import { confirmRequest } from '../tx/transfer-policy';
import { objArg } from '../utils';

export type PurchaseOptions = {
	extraArgs?: Record<string, any>;
};

/**
 * A Client that allows you to interact with kiosk.
 * Offers utilities to query kiosk, craft transactions to edit your own kiosk,
 * purchase, manage transfer policies, create new kiosks etc.
 */
export class KioskClient {
	client: SuiClient;
	network: Network;
	#rules: TransferPolicyRule[];
	selectedCap?: KioskOwnerCap;

	constructor(options: KioskClientOptions) {
		this.client = options.client;
		this.network = options.network;
		this.#rules = rules; // add all the default rules. WIP
	}

	/**
	 * Should be called before running any transactions that involve kiosk.
	 * @param cap The cap object, as returned from `getOwnedKiosks`.
	 */
	setSelectedCap(cap: KioskOwnerCap) {
		this.selectedCap = cap;
		return this;
	}

	// Someone would just have to create a `kiosk-client.ts` file in their project, initialize a KioskClient
	// and call the `addRuleResolver` function. Each rule has a `resolve` function.
	// The resolve function is automatically called on `purchaseAndResolve` function call.
	addRuleResolver(rule: TransferPolicyRule) {
		if (!this.#rules.find((x) => x.rule === rule.rule)) this.#rules.push(rule);
	}

	/**
	 * Creates a kiosk and returns both `Kiosk` and `KioskOwnerCap`.
	 * Helpful if we want to chain some actions before sharing + transferring the cap to the specified address.
	 */
	create(txb: TransactionBlock): [TransactionArgument, TransactionArgument] {
		let [kiosk, cap] = kioskTx.createKiosk(txb);
		return [kiosk, cap];
	}

	/**
	 * Single function way to create a kiosk, share it and transfer the cap to the specified address.
	 */
	createAndShare(txb: TransactionBlock, address: string) {
		let cap = kioskTx.createKioskAndShare(txb);
		txb.transferObjects([cap], txb.pure(address, 'address'));
	}

	/**
	 * Should be called only after `create` is called.
	 * It shares the kiosk & transfers the cap to the specified address.
	 */
	shareAndTransferCap(
		txb: TransactionBlock,
		kiosk: TransactionArgument,
		cap: TransactionArgument,
		address: string,
	) {
		kioskTx.shareKiosk(txb, kiosk);
		txb.transferObjects([cap], txb.pure(address, 'address'));
	}

	/**
	 * Wraps a kiosk transaction that depends on `kioskOwnerCap`.
	 * @param txb The Transaction Block
	 * @param ownerCap The `ownerCap` object as returned from the `getOwnedKiosk` function
	 * @param callback The function you want to execute with the ownerCap.
	 */
	async ownedKioskTx(
		txb: TransactionBlock,
		callback: (kiosk: TransactionArgument, capObject: TransactionArgument) => Promise<void>,
	): Promise<void> {
		this.#verifyCapIsSet();
		let [kiosk, capObject, returnPromise] = this.getOwnerCap(txb);

		await callback(kiosk, capObject);

		this.returnOwnerCap(txb, capObject, returnPromise);
	}

	/**
	 * Get an addresses's owned kiosks.
	 * @param address The address for which we want to retrieve the kiosks.
	 * @returns An Object containing all the `kioskOwnerCap` objects as well as the kioskIds.
	 */
	async getOwnedKiosks(address: string): Promise<OwnedKiosks> {
		let personalPackageId = personalKioskAddress[this.network];
		return getOwnedKiosks(this.client, address, {
			personalKioskType: personalPackageId
				? `${personalPackageId}::personal_kiosk::PersonalKioskCap`
				: '',
		});
	}

	/**
	 * Fetches the kiosk contents.
	 * @param kioskId
	 * @param options
	 * @returns
	 */
	async getKiosk(kioskId: string, options: FetchKioskOptions): Promise<KioskData> {
		return (
			await fetchKiosk(
				this.client,
				kioskId,
				{
					limit: 1000,
				},
				options,
			)
		).data;
	}

	/**
	 * Query the Transfer Policy(ies) for type `T`.
	 * @param itemType The Type we're querying for (E.g `0xMyAddress::hero::Hero`)
	 */
	async getTransferPolicies(itemType: string) {
		return queryTransferPolicy(this.client, itemType);
	}

	/**
	 * A function to purchase and resolve a transfer policy.
	 * If the transfer policy has the `lock` rule, the item is locked in the kiosk.
	 * Otherwise, the item is placed in the kiosk.
	 * @param txb The Transaction Block
	 * @param item The item {type, objectId, price}
	 * @param options Currently has `extraArgs`, which can be used for custom rule resolvers.
	 */
	async purchaseAndResolve(
		txb: TransactionBlock,
		itemType: string,
		itemId: string,
		price: string,
		kiosk: ObjectArgument,
		ownedKiosk: ObjectArgument,
		ownedKioskCap: ObjectArgument,
		options?: PurchaseOptions,
	): Promise<void> {
		this.#verifyCapIsSet();
		// Get a list of the transfer policies.
		let policies = await queryTransferPolicy(this.client, itemType);

		if (policies.length === 0) {
			throw new Error(
				`The type ${itemType} doesn't have a Transfer Policy so it can't be traded through kiosk.`,
			);
		}

		let policy = policies[0]; // we now pick the first one. We need to add an option to define which one.

		// Split the coin for the amount of the listing.
		const coin = txb.splitCoins(txb.gas, [txb.pure(price, 'u64')]);

		// initialize the purchase `kiosk::purchase`
		const [purchasedItem, transferRequest] = kioskTx.purchase(txb, itemType, kiosk, itemId, coin);

		let canTransferOutsideKiosk = true;

		for (let rule of policy.rules) {
			let ruleDefinition = this.#rules.find((x) => x.rule === rule);
			if (!ruleDefinition) throw new Error(`No resolver for the following rule: ${rule}.`);
			if (ruleDefinition.hasLockingRule) canTransferOutsideKiosk = false;

			ruleDefinition.resolveRuleFunction({
				packageId: ruleDefinition.packageId,
				txb,
				itemType,
				itemId,
				price,
				kiosk,
				policyId: policy.id,
				transferRequest,
				purchasedItem,
				ownedKiosk,
				ownedKioskCap,
				extraArgs: options?.extraArgs || {},
			});
		}

		confirmRequest(txb, itemType, policy.id, transferRequest);

		if (canTransferOutsideKiosk)
			this.place(txb, itemType, purchasedItem, ownedKiosk, ownedKioskCap);
	}

	/**
	 * A function to borrow an item from a kiosk & execute any function with it.
	 * Example: You could borrow a Fren out of a kiosk, attach an accessory (or mix), and return it.
	 */
	borrowTx(
		txb: TransactionBlock,
		itemType: string,
		itemId: string,
		kiosk: ObjectArgument,
		ownerCap: TransactionArgument,
		callback: (item: TransactionArgument) => Promise<void>,
	) {
		let [itemObj, promise] = kioskTx.borrowValue(txb, itemType, kiosk, ownerCap, itemId);

		callback(itemObj).finally(() => {
			kioskTx.returnValue(txb, itemType, kiosk, itemObj, promise);
		});
	}

	/**
	 * Borrows an item from the kiosk.
	 * This will fail if the item is listed for sale.
	 *
	 * Requires calling `return`.
	 */
	borrow(
		txb: TransactionBlock,
		itemType: string,
		itemId: string,
		kiosk: ObjectArgument,
		ownerCap: TransactionArgument,
	): [TransactionArgument, TransactionArgument] {
		let [itemObj, promise] = kioskTx.borrowValue(txb, itemType, kiosk, ownerCap, itemId);

		return [itemObj, promise];
	}

	/**
	 * Returns the item back to the kiosk.
	 * Accepts the parameters returned from the `borrow` function.
	 */
	return(
		txb: TransactionBlock,
		itemType: string,
		itemObj: TransactionArgument,
		promise: TransactionArgument,
		kiosk: ObjectArgument,
	) {
		kioskTx.returnValue(txb, itemType, kiosk, itemObj, promise);
	}

	/**
	 * A function to withdraw from kiosk
	 * @param txb The Transaction Block
	 * @param kiosk the Kiosk Object, ideally passed from the `ownedKioskTx` callback function!
	 * @param kioskCap the capObject, as returned from the `getOwnerCap` function.
	 * @param amount The amount we aim to withdraw.
	 */
	withdraw(
		txb: TransactionBlock,
		kiosk: ObjectArgument,
		kioskCap: TransactionArgument,
		amount?: string | bigint | number,
	): TransactionArgument {
		return kioskTx.withdrawFromKiosk(txb, kiosk, kioskCap, amount);
	}

	/**
	 * A function to place an item in the kiosk.
	 * @param txb The Transaction Block
	 * @param itemType The type `T` of the item
	 * @param item The ID or Transaction Argument of the item
	 * @param kiosk the Kiosk Object, ideally passed from the `ownedKioskTx` callback function!
	 * @param kioskCap the capObject, as returned from the `getOwnerCap` function.
	 */
	place(
		txb: TransactionBlock,
		itemType: string,
		item: ObjectArgument,
		kiosk: ObjectArgument,
		kioskCap: ObjectArgument,
	) {
		kioskTx.place(txb, itemType, kiosk, kioskCap, item);
	}

	/**
	 * A function to place an item in the kiosk and list it for sale in one transaction.
	 * @param txb The Transaction Block
	 * @param itemType The type `T` of the item
	 * @param item The ID or Transaction Argument of the item
	 * @param price The price in MIST
	 * @param kiosk the Kiosk Object, ideally passed from the `ownedKioskTx` callback function!
	 * @param kioskCap the capObject, as returned from the `getOwnerCap` function.
	 */
	placeAndList(
		txb: TransactionBlock,
		itemType: string,
		item: ObjectArgument,
		price: string | bigint,
		kiosk: ObjectArgument,
		kioskCap: ObjectArgument,
	) {
		kioskTx.placeAndList(txb, itemType, kiosk, kioskCap, item, price);
	}

	/**
	 * A function to list an item in the kiosk.
	 * @param txb The Transaction Block
	 * @param itemType The type `T` of the item
	 * @param itemId The ID of the item
	 * @param price The price in MIST
	 * @param kiosk the Kiosk Object, ideally passed from the `ownedKioskTx` callback function!
	 * @param kioskCap the capObject, as returned from the `getOwnerCap` function.
	 */
	list(
		txb: TransactionBlock,
		itemType: string,
		itemId: string,
		price: string | bigint,
		kiosk: ObjectArgument,
		kioskCap: ObjectArgument,
	) {
		kioskTx.list(txb, itemType, kiosk, kioskCap, itemId, price);
	}

	/**
	 * A function to delist an item from the kiosk.
	 * @param txb The Transaction Block
	 * @param itemType The type `T` of the item
	 * @param itemId The ID of the item
	 * @param kiosk the Kiosk, ideally passed from the `ownedKioskTx` callback function!
	 * @param kioskCap the KioskCap, ideally passed from the `ownedKioskTx` callback function!
	 */
	delist(
		txb: TransactionBlock,
		itemType: string,
		itemId: string,
		kiosk: ObjectArgument,
		kioskCap: ObjectArgument,
	) {
		kioskTx.delist(txb, itemType, kiosk, kioskCap, itemId);
	}

	/**
	 * A function to take an item from the kiosk. The transaction won't succeed if the item is listed or locked.
	 * @param txb The Transaction Block
	 * @param itemType The type `T` of the item
	 * @param itemId The ID of the item
	 * @param kiosk the Kiosk Object, ideally passed from the `ownedKioskTx` callback function!
	 * @param kioskCap the KioskCap, ideally passed from the `ownedKioskTx` callback function!
	 */
	take(
		txb: TransactionBlock,
		itemType: string,
		itemId: string,
		kiosk: ObjectArgument,
		kioskCap: ObjectArgument,
	): TransactionArgument {
		return kioskTx.take(txb, itemType, kiosk, kioskCap, itemId);
	}

	/**
	 * Transfer a non-locked/non-listed item to an address.
	 *
	 * @param txb The Transaction Block
	 * @param itemType The type `T` of the item
	 * @param itemId The ID of the item
	 * @param kiosk the Kiosk Object, ideally passed from the `ownedKioskTx` callback function!
	 * @param kioskCap the KioskCap, ideally passed from the `ownedKioskTx` callback function!
	 * @param address The destination address
	 */
	transfer(
		txb: TransactionBlock,
		itemType: string,
		itemId: string,
		kiosk: ObjectArgument,
		kioskCap: ObjectArgument,
		address: string,
	) {
		const item = this.take(txb, itemType, itemId, kiosk, kioskCap);
		txb.transferObjects([item], txb.pure(address, 'address'));
	}

	/**
	 * A function to take lock an item in the kiosk.
	 * @param txb The Transaction Block
	 * @param itemType The type `T` of the item
	 * @param itemId The ID of the item
	 * @param policy The Policy ID or Transaction Argument for item T
	 * @param kiosk the Kiosk Object, ideally passed from the `ownedKioskTx` callback function!
	 * @param kioskCap the KioskCap, ideally passed from the `ownedKioskTx` callback function!
	 */
	lock(
		txb: TransactionBlock,
		itemType: string,
		itemId: string,
		policy: ObjectArgument,
		kiosk: ObjectArgument,
		kioskCap: ObjectArgument,
	) {
		kioskTx.lock(txb, itemType, kiosk, kioskCap, policy, itemId);
	}

	/**
	 * Converts a kiosk to a Personal (Soulbound) Kiosk.
	 * @param txb The Transaction Block
	 * @param kiosk (Optional) The Kiosk Id or Object
	 * @param ownerCap (Optional) The Kiosk Owner Cap Object. If not passed, it will use the selectedCap's one.
	 */
	convertToPersonal(txb: TransactionBlock, kiosk?: ObjectArgument, ownerCap?: ObjectArgument) {
		if (!kiosk || !ownerCap) this.#verifyCapIsSet();
		convertToPersonalTx(
			txb,
			kiosk || this.selectedCap!.kioskId,
			ownerCap || this.selectedCap!.objectId,
			personalKioskAddress[this.network],
		);
	}

	/**
	 * A function to get a transaction parameter for the kiosk.
	 * @param txbb The Transaction Block
	 * @returns An array [kioskOwnerCap, promise]. If there's a promise, you need to call `returnOwnerCap` after using the cap.
	 */
	getOwnerCap(
		txb: TransactionBlock,
	): [TransactionArgument, TransactionArgument, TransactionArgument | undefined] {
		this.#verifyCapIsSet();
		if (!this.selectedCap!.isPersonal)
			return [
				objArg(txb, this.selectedCap!.kioskId),
				objArg(txb, this.selectedCap!.objectId),
				undefined,
			];

		const [capObject, promise] = txb.moveCall({
			target: `${personalKioskAddress[this.network]}::personal_kiosk::borrow_val`,
			arguments: [txb.object(this.selectedCap!.objectId)],
		});

		return [objArg(txb, this.selectedCap!.kioskId), capObject, promise];
	}

	/**
	 * A function to return the `kioskOwnerCap` back to `PersonalKiosk` wrapper.
	 * @param txb The Transaction Block
	 * @param capObject The borrowed `KioskOwnerCap`
	 * @param promise The promise that the cap would return
	 */
	returnOwnerCap(
		txb: TransactionBlock,
		capObject: ObjectArgument,
		promise?: TransactionArgument | undefined,
	) {
		this.#verifyCapIsSet();
		if (!this.selectedCap!.isPersonal || !promise) return;

		txb.moveCall({
			target: `${personalKioskAddress[this.network]}::personal_kiosk::return_val`,
			arguments: [txb.object(this.selectedCap!.objectId), objArg(txb, capObject), promise],
		});
	}

	/**
	 * This helps check whether we have the `selectedCap` set.
	 */
	#verifyCapIsSet() {
		if (!this.selectedCap)
			throw new Error('You need to call `setSelectedCap` before calling this function.');
	}
}
