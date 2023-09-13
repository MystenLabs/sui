// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import { TransactionArgument, TransactionBlock } from '@mysten/sui.js/transactions';
import { KioskClient } from './kiosk-client';
import {
	ItemId,
	ItemReference,
	ItemValue,
	KioskOwnerCap,
	ObjectArgument,
	Price,
	PurchaseOptions,
} from '../types';
import { objArg } from '../utils';
import { PERSONAL_KIOSK_RULE_ADDRESS } from '../constants';
import * as kioskTx from '../tx/kiosk';
import { confirmRequest } from '../tx/transfer-policy';
import { convertToPersonalTx } from '../tx/personal-kiosk';

export type KioskTransactionParams = {
	/** The TransactionBlock for this run */
	txb: TransactionBlock;
	/**
	 * You can create a new KioskClient by calling `new KioskClient()`
	 */
	kioskClient: KioskClient;
	/**
	 * You can optionally pass in the `cap` as returned
	 * from `kioskClient.getOwnedKiosks` when initializing the client
	 * Otherwise, you can set it by calling `kioskTransaction.setCap()`
	 */
	cap?: KioskOwnerCap;
};

/**
 * A helper for building transactions that involve kiosk.
 */
export class KioskTransaction {
	txb: TransactionBlock;
	kioskClient: KioskClient;
	cap?: KioskOwnerCap;
	kiosk?: TransactionArgument;
	kioskCap?: TransactionArgument;
	#promise?: TransactionArgument | undefined;

	constructor({ txb, kioskClient, cap }: KioskTransactionParams) {
		this.txb = txb;
		this.kioskClient = kioskClient;

		if (cap) this.setCap(cap);
		return this;
	}

	/**
	 * Creates a kiosk and returns both `Kiosk` and `KioskOwnerCap`.
	 * Helpful if we want to chain some actions before sharing + transferring the cap to the specified address.
	 */
	create() {
		this.cap = undefined; // reset cap, as we now operate on the newly created kiosk.
		const [kiosk, cap] = kioskTx.createKiosk(this.txb);
		this.kiosk = kiosk;
		this.kioskCap = cap;
		return this;
	}

	/**
	 * Creates a personal kiosk & shares it.
	 * The `PersonalKioskCap` is transferred to the signer.
	 */
	createPersonal() {
		this.create().convertToPersonal();
		kioskTx.shareKiosk(this.txb, this.kiosk!);
	}

	/**
	 * Converts a kiosk to a Personal (Soulbound) Kiosk.
	 * Requires initialization by either calling `ktxb.create()` or `ktxb.setCap()`.
	 */
	convertToPersonal() {
		this.#validateKioskIsSet();
		if (this.cap && this.cap.isPersonal) throw new Error('This kiosk is already personal');

		convertToPersonalTx(
			this.txb,
			this.kiosk!,
			this.kioskCap!,
			PERSONAL_KIOSK_RULE_ADDRESS[this.kioskClient.network],
		);
	}

	/**
	 * Single function way to create a kiosk, share it and transfer the cap to the specified address.
	 */
	createAndShare(address: string) {
		const cap = kioskTx.createKioskAndShare(this.txb);
		this.txb.transferObjects([cap], this.txb.pure(address, 'address'));
	}

	/**
	 * Should be called only after `create` is called.
	 * It shares the kiosk & transfers the cap to the specified address.
	 */
	shareAndTransferCap(address: string) {
		this.#validateKioskIsSet();
		kioskTx.shareKiosk(this.txb, this.kiosk!);
		this.txb.transferObjects([this.kioskCap!], this.txb.pure(address, 'address'));
	}

	/**
	 * A function to borrow an item from a kiosk & execute any function with it.
	 * Example: You could borrow a Fren out of a kiosk, attach an accessory (or mix), and return it.
	 */
	borrowTx({ itemType, itemId }: ItemId, callback: (item: TransactionArgument) => Promise<void>) {
		this.#validateKioskIsSet();
		const [itemObj, promise] = kioskTx.borrowValue(
			this.txb,
			itemType,
			this.kiosk!,
			this.kioskCap!,
			itemId,
		);

		callback(itemObj).finally(() => {
			this.return({ itemType, item: itemObj, promise });
		});
	}

	/**
	 * Borrows an item from the kiosk.
	 * This will fail if the item is listed for sale.
	 *
	 * Requires calling `return`.
	 */
	borrow({ itemType, itemId }: ItemId): [TransactionArgument, TransactionArgument] {
		this.#validateKioskIsSet();
		const [itemObj, promise] = kioskTx.borrowValue(
			this.txb,
			itemType,
			this.kiosk!,
			this.kioskCap!,
			itemId,
		);

		return [itemObj, promise];
	}

	/**
	 * Returns the item back to the kiosk.
	 * Accepts the parameters returned from the `borrow` function.
	 */
	return({ itemType, item, promise }: ItemValue & { promise: TransactionArgument }) {
		this.#validateKioskIsSet();
		kioskTx.returnValue(this.txb, itemType, this.kiosk!, item, promise);
		return this;
	}

	/**
	 * A function to withdraw from kiosk
	 * @param address Where to trasnfer the coin.
	 * @param amount The amount we aim to withdraw.
	 */
	withdraw(address: string, amount?: string | bigint | number) {
		this.#validateKioskIsSet();
		const coin = kioskTx.withdrawFromKiosk(this.txb, this.kiosk!, this.kioskCap!, amount);
		this.txb.transferObjects([coin], this.txb.pure(address, 'address'));
		return this;
	}

	/**
	 * A function to place an item in the kiosk.
	 * @param itemType The type `T` of the item
	 * @param item The ID or Transaction Argument of the item
	 */
	place({ itemType, item }: ItemReference) {
		this.#validateKioskIsSet();
		kioskTx.place(this.txb, itemType, this.kiosk!, this.kioskCap!, item);
		return this;
	}

	/**
	 * A function to place an item in the kiosk and list it for sale in one transaction.
	 * @param itemType The type `T` of the item
	 * @param item The ID or Transaction Argument of the item
	 * @param price The price in MIST
	 */
	placeAndList({ itemType, item, price }: ItemReference & Price) {
		this.#validateKioskIsSet();
		kioskTx.placeAndList(this.txb, itemType, this.kiosk!, this.kioskCap!, item, price);
		return this;
	}

	/**
	 * A function to list an item in the kiosk.
	 * @param itemType The type `T` of the item
	 * @param itemId The ID of the item
	 * @param price The price in MIST
	 */
	list({ itemType, itemId, price }: ItemId & { price: string | bigint }) {
		this.#validateKioskIsSet();
		kioskTx.list(this.txb, itemType, this.kiosk!, this.kioskCap!, itemId, price);
		return this;
	}

	/**
	 * A function to delist an item from the kiosk.
	 * @param itemType The type `T` of the item
	 * @param itemId The ID of the item
	 */
	delist({ itemType, itemId }: ItemId) {
		this.#validateKioskIsSet();
		kioskTx.delist(this.txb, itemType, this.kiosk!, this.kioskCap!, itemId);
		return this;
	}

	/**
	 * A function to take an item from the kiosk. The transaction won't succeed if the item is listed or locked.

	 * @param itemType The type `T` of the item
	 * @param itemId The ID of the item
	 */
	take({ itemType, itemId }: ItemId): TransactionArgument {
		this.#validateKioskIsSet();
		return kioskTx.take(this.txb, itemType, this.kiosk!, this.kioskCap!, itemId);
	}

	/**
	 * Transfer a non-locked/non-listed item to an address.
	 *

	 * @param itemType The type `T` of the item
	 * @param itemId The ID of the item
	 * @param address The destination address
	 */
	transfer({ itemType, itemId, address }: ItemId & { address: string }) {
		this.#validateKioskIsSet();
		const item = this.take({ itemType, itemId });
		this.txb.transferObjects([item], this.txb.pure(address, 'address'));
		return this;
	}

	/**
	 * A function to take lock an item in the kiosk.

	 * @param itemType The type `T` of the item
	 * @param itemId The ID of the item
	 * @param policy The Policy ID or Transaction Argument for item T
	 */
	lock({ itemType, itemId, policy }: ItemId & { policy: ObjectArgument }) {
		this.#validateKioskIsSet();
		kioskTx.lock(this.txb, itemType, this.kiosk!, this.kioskCap!, policy, itemId);
		return this;
	}

	/**
	 * A function to purchase and resolve a transfer policy.
	 * If the transfer policy has the `lock` rule, the item is locked in the kiosk.
	 * Otherwise, the item is placed in the kiosk.
	 * @param itemType The type of the item
	 * @param itemId The id of the item
	 * @param price The price of the specified item
	 * @param sellerKiosk The kiosk which is selling the item. Can be an id or an object argument.
	 * @param extraArgs Used to pass arguments for custom rule resolvers.
	 */
	async purchaseAndResolve({
		itemType,
		itemId,
		price,
		sellerKiosk,
		extraArgs,
	}: ItemId & Price & { sellerKiosk: ObjectArgument } & PurchaseOptions) {
		this.#validateKioskIsSet();
		// Get a list of the transfer policies.
		const policies = await this.kioskClient.getTransferPolicies({ type: itemType });

		if (policies.length === 0) {
			throw new Error(
				`The type ${itemType} doesn't have a Transfer Policy so it can't be traded through kiosk.`,
			);
		}

		const policy = policies[0]; // we now pick the first one. We need to add an option to define which one.

		// Split the coin for the amount of the listing.
		const coin = this.txb.splitCoins(this.txb.gas, [this.txb.pure(price, 'u64')]);

		// initialize the purchase `kiosk::purchase`
		const [purchasedItem, transferRequest] = kioskTx.purchase(
			this.txb,
			itemType,
			sellerKiosk,
			itemId,
			coin,
		);

		let canTransferOutsideKiosk = true;

		for (const rule of policy.rules) {
			const ruleDefinition = this.kioskClient.rules.find((x) => x.rule === rule);
			if (!ruleDefinition) throw new Error(`No resolver for the following rule: ${rule}.`);
			if (ruleDefinition.hasLockingRule) canTransferOutsideKiosk = false;

			ruleDefinition.resolveRuleFunction({
				packageId: ruleDefinition.packageId,
				txb: this.txb,
				itemType,
				itemId,
				price: price.toString(),
				sellerKiosk,
				policyId: policy.id,
				transferRequest,
				purchasedItem,
				kiosk: this.kiosk!,
				kioskCap: this.kioskCap!,
				extraArgs: extraArgs || {},
			});
		}

		confirmRequest(this.txb, itemType, policy.id, transferRequest);

		if (canTransferOutsideKiosk) this.place({ itemType, item: purchasedItem });

		return this;
	}

	/**
	 * A function to setup the client using an existing `ownerCap`,
	 * as return from the `kioskClient.getOwnedKiosks` function.
	 * @param cap `KioskOwnerCap` object as returned from `getOwnedKiosks` SDK call.
	 */
	setCap(cap: KioskOwnerCap) {
		this.kiosk = objArg(this.txb, cap.kioskId);
		console.log(this.kiosk);
		if (!cap.isPersonal) {
			this.kioskCap = objArg(this.txb, cap.objectId);
			return;
		}

		const [kioskCap, promise] = this.txb.moveCall({
			target: `${
				PERSONAL_KIOSK_RULE_ADDRESS[this.kioskClient.network]
			}::personal_kiosk::borrow_val`,
			arguments: [objArg(this.txb, cap.objectId)],
		});

		this.cap = cap;
		this.kioskCap = kioskCap;
		this.#promise = promise;

		return this;
	}

	/**
	 *	A function that wraps up the kiosk building txb & returns the `kioskOwnerCap` back to the
	 *  `PersonalKioskCap`, in case we are operating on a personal kiosk.
	 */
	wrap() {
		this.#validateKioskIsSet();
		if (!this.cap || !this.#promise || !this.cap.isPersonal) return;

		this.txb.moveCall({
			target: `${
				PERSONAL_KIOSK_RULE_ADDRESS[this.kioskClient.network]
			}::personal_kiosk::return_val`,
			arguments: [
				objArg(this.txb, this.cap.objectId),
				objArg(this.txb, this.kioskCap!),
				this.#promise,
			],
		});
	}

	// Some getters
	/*
	 * Returns the active transaction's kiosk, or undefined if `setCap` or `create()` hasn't been called yet.
	 */
	getKiosk() {
		return this.kiosk;
	}

	/*
	 * Returns the active transaction's kioskOwnerCap, or undefined if `setCap` or `create()` hasn't been called yet.
	 */
	getKioskCap() {
		return this.kioskCap;
	}
	/*
	 * If operating over an existing kiosk, returns the active cap
	 */
	getCap() {
		return this.cap;
	}

	#validateKioskIsSet() {
		if (!this.kiosk || !this.kioskCap)
			throw new Error(
				'You need to initialize the client by either supplying an existing owner cap or by creating a new by calling `.create()`',
			);
	}
}
