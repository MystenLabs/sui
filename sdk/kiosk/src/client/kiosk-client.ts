// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import { type SuiClient } from '@mysten/sui.js/src/client';
import { fetchKiosk, getOwnedKiosks } from '../query/kiosk';
import {
	type FetchKioskOptions,
	KioskData,
	KioskItem,
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
	extraArgs: Record<string, ObjectArgument>;
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

	constructor(options: KioskClientOptions) {
		this.client = options.client;
		this.network = options.network;
		this.#rules = rules; // add all the default rules. WIP
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
		ownerCap: KioskOwnerCap,
		callback: (
			tx: TransactionBlock,
			kiosk: ObjectArgument,
			capObject: ObjectArgument,
		) => Promise<void>,
	): Promise<void> {
		let [capObject, returnPromise] = this.getOwnerCap(tx, ownerCap);

		await callback(tx, ownerCap.kioskId, capObject);

		if (returnPromise) this.returnOwnerCap(tx, ownerCap, capObject, returnPromise);
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
	async fetchKiosk(kioskId: string, options: FetchKioskOptions): Promise<KioskData> {
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
	 * A function to purchase and resolve a transfer policy.
	 * If the transfer policy has the `lock` rule, the item is locked in the kiosk.
	 * Otherwise, the item is placed in the kiosk.
	 * @param tx The Transaction Block
	 * @param item The item {type, objectId, price}
	 * @param options Currently has `extraArgs`, which can be used for custom rule resolvers.
	 */
	async purchaseAndResolve(
		tx: TransactionBlock,
		item: KioskItem,
		kiosk: ObjectArgument,
		ownedKiosk: ObjectArgument,
		ownedKioskCap: ObjectArgument,
		options?: PurchaseOptions,
	): Promise<void> {
		// Get a list of the transfer policies.
		let policies = await queryTransferPolicy(this.client, item.type);

		if (policies.length === 0) {
			throw new Error(
				`The type ${item.type} doesn't have a Transfer Policy so it can't be traded through kiosk.`,
			);
		}

		let policy = policies[0]; // we now pick the first one. We need to add an option to define which one.

		// if we don't pass the listing or the listing doens't have a price, return.
		if (item.listing?.price === undefined || typeof item.listing.price !== 'string')
			throw new Error(`Price of the listing is not supplied.`);

		// Split the coin for the amount of the listing.
		const coin = tx.splitCoins(tx.gas, [tx.pure(item.listing.price, 'u64')]);

		// initialize the purchase `kiosk::purchase`
		const [purchasedItem, transferRequest] = kioskTx.purchase(
			tx,
			item.type,
			kiosk,
			item.objectId,
			coin,
		);

		let canTransferOutsideKiosk = true;

		for (let rule of policy.rules) {
			let ruleDefinition = this.#rules.find((x) => x.rule === rule);
			if (!ruleDefinition) throw new Error(`No resolver for the following rule: ${rule}.`);
			if (ruleDefinition.hasLockingRule) canTransferOutsideKiosk = false;

			console.log('Trying to resolve... ' + rule);

			ruleDefinition.resolveRuleFunction({
				packageId: ruleDefinition.packageId,
				tx: tx,
				item,
				kiosk,
				policyId: policy.id,
				transferRequest,
				purchasedItem,
				ownedKiosk,
				ownedKioskCap,
				extraArgs: options?.extraArgs,
			});
		}

		confirmRequest(tx, item.type, policy.id, transferRequest);

		if (canTransferOutsideKiosk)
			this.place(tx, item.type, purchasedItem, ownedKiosk, ownedKioskCap);
	}

	/**
	 * A function to borrow an item from a kiosk & execute any function with it.
	 * Example: You could borrow a Fren out of a kiosk, attach an accessory (or mix), and return it.
	 */
	borrowTx(
		tx: TransactionBlock,
		item: KioskItem,
		kioskId: ObjectArgument,
		ownerCap: TransactionArgument,
		callback: (tx: TransactionBlock, item: TransactionArgument) => Promise<void>,
	) {
		let [itemObj, promise] = kioskTx.borrowValue(tx, item.type, kioskId, ownerCap, item.objectId);

		callback(tx, itemObj).finally(() => {
			kioskTx.returnValue(tx, item.type, kioskId, itemObj, promise);
		});
	}

	/**
	 * A function to withdraw from kiosk
	 * @param tx The Transaction Block
	 * @param ownerCap The KioskOwnerCap object that we have received from the SDK `getOwnedKiosks` call.
	 * @param amount The amount we aim to withdraw.
	 */
	withdraw(
		tx: TransactionBlock,
		kioskId: ObjectArgument,
		ownerCap: ObjectArgument,
		amount: string | bigint | null,
	): TransactionArgument {
		return kioskTx.withdrawFromKiosk(tx, kioskId, ownerCap, amount);
	}

	/**
	 * A function to place an item in the kiosk.
	 * @param tx The Transaction Block
	 * @param item The item {type, objectId} we want to delist
	 * @param kioskId the Kiosk Id, ideally passed from the `ownedKioskTx` callback function!
	 * @param kioskCap the KioskCap, ideally passed from the `ownedKioskTx` callback function!
	 */
	place(
		tx: TransactionBlock,
		itemType: string,
		item: ObjectArgument,
		kioskId: ObjectArgument,
		kioskCap: ObjectArgument,
	) {
		kioskTx.place(tx, itemType, kioskId, kioskCap, item);
	}

	/**
	 * A function to place an item in the kiosk and list it for sale in one transaction.
	 * @param tx The Transaction Block
	 * @param item The item {type, objectId} we want to delist
	 * @param price The price in MIST
	 * @param kioskId the Kiosk Id, ideally passed from the `ownedKioskTx` callback function!
	 * @param kioskCap the KioskCap, ideally passed from the `ownedKioskTx` callback function!
	 */
	placeAndList(
		tx: TransactionBlock,
		itemType: string,
		item: ObjectArgument,
		price: string | bigint,
		kioskId: ObjectArgument,
		kioskCap: ObjectArgument,
	): void {
		kioskTx.placeAndList(tx, itemType, kioskId, kioskCap, item, price);
	}

	/**
	 * A function to list an item in the kiosk.
	 * @param tx The Transaction Block
	 * @param item The item {type, objectId} we want to delist
	 * @param price The price in MIST
	 * @param kiosk the Kiosk, ideally passed from the `ownedKioskTx` callback function!
	 * @param kioskCap the KioskCap, ideally passed from the `ownedKioskTx` callback function!
	 */
	list(
		tx: TransactionBlock,
		itemType: string,
		itemId: string,
		price: string | bigint,
		kiosk: ObjectArgument,
		kioskCap: ObjectArgument,
	): void {
		kioskTx.list(tx, itemType, kiosk, kioskCap, itemId, price);
	}

	/**
	 * A function to delist an item from the kiosk.
	 * @param tx The Transaction Block
	 * @param item The item {type, objectId} we want to delist
	 * @param kiosk the Kiosk, ideally passed from the `ownedKioskTx` callback function!
	 * @param kioskCap the KioskCap, ideally passed from the `ownedKioskTx` callback function!
	 */
	delist(
		tx: TransactionBlock,
		itemType: string,
		itemId: string,
		kiosk: ObjectArgument,
		kioskCap: ObjectArgument,
	): void {
		kioskTx.delist(tx, itemType, kiosk, kioskCap, itemId);
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
		kiosk: ObjectArgument,
		kioskCap: ObjectArgument,
	): TransactionArgument {
		return kioskTx.take(tx, itemType, kiosk, kioskCap, itemId);
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
	 * @param ownerCap The KioskOwnerCap object that we have received from the SDK `getOwnedKiosks` call.
	 * @returns An array [kioskOwnerCap, promise]. If there's a promise, you need to call `returnOwnerCap` after using the cap.
	 */
	getOwnerCap(
		tx: TransactionBlock,
		ownerCap: KioskOwnerCap,
	): [ObjectArgument, TransactionArgument | undefined] {
		if (!ownerCap.isPersonal) return [ownerCap.objectId, undefined];

		const [capObject, promise] = tx.moveCall({
			target: `${personalKioskAddress[this.network]}::personal_kiosk::borrow_val`,
			arguments: [tx.object(ownerCap.objectId)],
		});

		return [capObject, promise];
	}

	/**
	 * A function to return the `kioskOwnerCap` back to `PersonalKiosk` wrapper.
	 * @param tx The Transaction Block
	 * @param ownerCap The original ownerCap as returned from SDK, to find the wrapper's object id.
	 * @param capObject The borrowed `KioskOwnerCap`
	 * @param promise The promise that the cap would return
	 */
	returnOwnerCap(
		tx: TransactionBlock,
		ownerCap: KioskOwnerCap,
		capObject: ObjectArgument,
		promise?: TransactionArgument,
	): void {
		if (!ownerCap.isPersonal || !promise) return;

		tx.moveCall({
			target: `${personalKioskAddress[this.network]}::personal_kiosk::return_val`,
			arguments: [tx.object(ownerCap.objectId), objArg(tx, capObject), promise],
		});
	}
}
