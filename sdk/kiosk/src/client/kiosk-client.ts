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
	extraArgs?: Record<string, ObjectArgument>;
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
	}

	// Someone would just have to create a `kiosk-client.ts` file in their project, initialize a KioskClient
	// and call the `addRuleResolver` function. Each rule has a `resolve` function.
	// The resolve function is automatically called on `purchaseAndResolve` function call.
	addRuleResolver(rule: TransferPolicyRule) {
		if (!this.#rules.find((x) => x.rule === rule.rule)) this.#rules.push(rule);
	}

	/**
	 * Wraps a kiosk transaction that depends on `kioskOwnerCap`.
	 * @param tx The Transaction Block
	 * @param ownerCap The `ownerCap` object as returned from the `getOwnedKiosk` function
	 * @param callback The function you want to execute with the ownerCap.
	 */
	async ownedKioskTx(
		tx: TransactionBlock,
		callback: (capObject: TransactionArgument) => Promise<void>,
	): Promise<void> {
		this.#verifyCapIsSet();
		let [capObject, returnPromise] = this.getOwnerCap(tx);

		await callback(capObject);

		this.returnOwnerCap(tx, capObject, returnPromise);
	}

	#verifyCapIsSet() {
		if (!this.selectedCap)
			throw new Error('You need to call `setSelectedCap` before calling this function.');
	}
	/**
	 * Get an addresses's owned kiosks.
	 * @param address The address for which we want to retrieve the kiosks.
	 * @returns An Object containing all the `kioskOwnerCap` objects as well as the kioskIds.
	 */
	async getOwnedKiosks(address: string): Promise<OwnedKiosks> {
		return getOwnedKiosks(this.client, address, {
			personalKioskType: `${personalKioskAddress[this.network]}::personal_kiosk::PersonalKioskCap`,
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
	 * @param tx The Transaction Block
	 * @param item The item {type, objectId, price}
	 * @param options Currently has `extraArgs`, which can be used for custom rule resolvers.
	 */
	async purchaseAndResolve(
		tx: TransactionBlock,
		itemType: string,
		itemId: string,
		price: string,
		kiosk: ObjectArgument,
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
		const coin = tx.splitCoins(tx.gas, [tx.pure(price, 'u64')]);

		// initialize the purchase `kiosk::purchase`
		const [purchasedItem, transferRequest] = kioskTx.purchase(tx, itemType, kiosk, itemId, coin);

		let canTransferOutsideKiosk = true;

		for (let rule of policy.rules) {
			let ruleDefinition = this.#rules.find((x) => x.rule === rule);
			if (!ruleDefinition) throw new Error(`No resolver for the following rule: ${rule}.`);
			if (ruleDefinition.hasLockingRule) canTransferOutsideKiosk = false;

			console.log('Trying to resolve... ' + rule);

			ruleDefinition.resolveRuleFunction({
				packageId: ruleDefinition.packageId,
				tx: tx,
				itemType,
				itemId,
				price,
				kiosk,
				policyId: policy.id,
				transferRequest,
				purchasedItem,
				ownedKiosk: this.selectedCap!.kioskId,
				ownedKioskCap,
				extraArgs: options?.extraArgs,
			});
		}

		confirmRequest(tx, itemType, policy.id, transferRequest);

		if (canTransferOutsideKiosk) this.place(tx, itemType, purchasedItem, ownedKioskCap);
	}

	/**
	 * A function to borrow an item from a kiosk & execute any function with it.
	 * Example: You could borrow a Fren out of a kiosk, attach an accessory (or mix), and return it.
	 */
	borrowTx(
		tx: TransactionBlock,
		itemType: string,
		itemId: string,
		ownerCap: TransactionArgument,
		callback: (item: TransactionArgument) => Promise<void>,
	) {
		this.#verifyCapIsSet();
		let [itemObj, promise] = kioskTx.borrowValue(
			tx,
			itemType,
			this.selectedCap!.kioskId,
			ownerCap,
			itemId,
		);

		callback(itemObj).finally(() => {
			kioskTx.returnValue(tx, itemType, this.selectedCap!.kioskId, itemObj, promise);
		});
	}

	/**
	 * Borrows an item from the kiosk.
	 * This will fail if the item is listed for sale.
	 *
	 * Requires calling `return`.
	 */
	borrow(
		tx: TransactionBlock,
		itemType: string,
		itemId: string,
		ownerCap: TransactionArgument,
	): [TransactionArgument, TransactionArgument] {
		this.#verifyCapIsSet();
		let [itemObj, promise] = kioskTx.borrowValue(
			tx,
			itemType,
			this.selectedCap!.kioskId,
			ownerCap,
			itemId,
		);

		return [itemObj, promise];
	}

	/**
	 * Returns the item back to the kiosk.
	 * Accepts the parameters returned from the `borrow` function.
	 */
	return(
		tx: TransactionBlock,
		itemType: string,
		itemObj: TransactionArgument,
		promise: TransactionArgument,
	) {
		this.#verifyCapIsSet();
		kioskTx.returnValue(tx, itemType, this.selectedCap!.kioskId, itemObj, promise);
	}

	/**
	 * A function to withdraw from kiosk
	 * @param tx The Transaction Block
	 * @param ownerCap The KioskOwnerCap object that we have received from the SDK `getOwnedKiosks` call.
	 * @param amount The amount we aim to withdraw.
	 */
	withdraw(
		tx: TransactionBlock,
		ownerCap: TransactionArgument,
		amount?: string | bigint | null,
	): TransactionArgument {
		this.#verifyCapIsSet();
		return kioskTx.withdrawFromKiosk(tx, this.selectedCap!.kioskId, ownerCap, amount);
	}

	/**
	 * A function to place an item in the kiosk.
	 * @param tx The Transaction Block
	 * @param item The item {type, objectId} we want to delist
	 * @param kioskCap the capObject, as returned from the `getOwnerCap` function.
	 */
	place(tx: TransactionBlock, itemType: string, item: ObjectArgument, kioskCap: ObjectArgument) {
		this.#verifyCapIsSet();
		kioskTx.place(tx, itemType, this.selectedCap!.kioskId, kioskCap, item);
	}

	/**
	 * A function to place an item in the kiosk and list it for sale in one transaction.
	 * @param tx The Transaction Block
	 * @param price The price in MIST
	 * @param kioskCap the capObject, as returned from the `getOwnerCap` function.
	 */
	placeAndList(
		tx: TransactionBlock,
		itemType: string,
		item: ObjectArgument,
		price: string | bigint,
		kioskCap: ObjectArgument,
	): void {
		this.#verifyCapIsSet();
		kioskTx.placeAndList(tx, itemType, this.selectedCap!.kioskId, kioskCap, item, price);
	}

	/**
	 * A function to list an item in the kiosk.
	 * @param tx The Transaction Block
	 * @param item The item {type, objectId} we want to delist
	 * @param price The price in MIST
	 * @param kioskCap the capObject, as returned from the `getOwnerCap` function.
	 */
	list(
		tx: TransactionBlock,
		itemType: string,
		itemId: string,
		price: string | bigint,
		kioskCap: ObjectArgument,
	): void {
		this.#verifyCapIsSet();
		kioskTx.list(tx, itemType, this.selectedCap!.kioskId, kioskCap, itemId, price);
	}

	/**
	 * A function to delist an item from the kiosk.
	 * @param tx The Transaction Block
	 * @param item The item {type, objectId} we want to delist
	 * @param kiosk the Kiosk, ideally passed from the `ownedKioskTx` callback function!
	 * @param kioskCap the KioskCap, ideally passed from the `ownedKioskTx` callback function!
	 */
	delist(tx: TransactionBlock, itemType: string, itemId: string, kioskCap: ObjectArgument): void {
		this.#verifyCapIsSet();
		kioskTx.delist(tx, itemType, this.selectedCap!.kioskId, kioskCap, itemId);
	}

	/**
	 * A function to take an item from the kiosk. The transaction won't succeed if the item is listed or locked.
	 * @param tx The Transaction Block
	 * @param item The item {type, objectId} we want to delist
	 * @param kiosk the Kiosk Id, ideally passed from the `ownedKioskTx` callback function!
	 * @param kioskCap the KioskCap, ideally passed from the `ownedKioskTx` callback function!
	 */
	take(
		tx: TransactionBlock,
		itemType: string,
		itemId: string,
		kioskCap: ObjectArgument,
	): TransactionArgument {
		this.#verifyCapIsSet();
		return kioskTx.take(tx, itemType, this.selectedCap!.kioskId, kioskCap, itemId);
	}

	/**
	 * Converts a kiosk to a Personal (Soulbound) Kiosk.
	 * @param tx The Transaction Block
	 * @param kiosk The Kiosk Id
	 * @param ownerCap The Kiosk Owner Cap Object
	 * @param address The address to transfer the cap.
	 */
	convertToPersonal(tx: TransactionBlock, kiosk: ObjectArgument, ownerCap: ObjectArgument): void {
		convertToPersonalTx(tx, kiosk, ownerCap, personalKioskAddress[this.network]);
	}

	/**
	 * A function to get a transaction parameter for the kiosk.
	 * @param tx The Transaction Block
	 * @returns An array [kioskOwnerCap, promise]. If there's a promise, you need to call `returnOwnerCap` after using the cap.
	 */
	getOwnerCap(tx: TransactionBlock): [TransactionArgument, TransactionArgument | undefined] {
		this.#verifyCapIsSet();
		if (!this.selectedCap!.isPersonal) return [tx.object(this.selectedCap!.objectId), undefined];

		const [capObject, promise] = tx.moveCall({
			target: `${personalKioskAddress[this.network]}::personal_kiosk::borrow_val`,
			arguments: [tx.object(this.selectedCap!.objectId)],
		});

		return [capObject, promise];
	}

	/**
	 * A function to return the `kioskOwnerCap` back to `PersonalKiosk` wrapper.
	 * @param tx The Transaction Block
	 * @param capObject The borrowed `KioskOwnerCap`
	 * @param promise The promise that the cap would return
	 */
	returnOwnerCap(
		tx: TransactionBlock,
		capObject: ObjectArgument,
		promise?: TransactionArgument | undefined,
	): void {
		this.#verifyCapIsSet();
		if (!this.selectedCap!.isPersonal || !promise) return;

		tx.moveCall({
			target: `${personalKioskAddress[this.network]}::personal_kiosk::return_val`,
			arguments: [tx.object(this.selectedCap!.objectId), objArg(tx, capObject), promise],
		});
	}
}
