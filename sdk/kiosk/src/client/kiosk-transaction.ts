// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import type {
	Transaction,
	TransactionArgument,
	TransactionObjectArgument,
} from '@mysten/sui/transactions';

import * as kioskTx from '../tx/kiosk.js';
import { convertToPersonalTx, transferPersonalCapTx } from '../tx/personal-kiosk.js';
import { confirmRequest } from '../tx/transfer-policy.js';
import type {
	ItemId,
	ItemReference,
	ItemValue,
	KioskOwnerCap,
	ObjectArgument,
	Price,
	PurchaseOptions,
} from '../types/index.js';
import { getNormalizedRuleType } from '../utils.js';
import type { KioskClient } from './kiosk-client.js';

export type KioskTransactionParams = {
	/** The Transaction for this run */
	transaction: Transaction;

	/** @deprecated use transaction instead */
	transactionBlock?: Transaction;
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
	transaction: Transaction;
	kioskClient: KioskClient;
	kiosk?: TransactionObjectArgument;
	kioskCap?: TransactionObjectArgument;
	// If we're pending `share` of a new kiosk, `finalize()` will share it.
	#pendingShare?: boolean;
	// If we're pending transferring of the cap, `finalize()` will either error or transfer the cap if it's a new personal.
	#pendingTransfer?: boolean;
	// The promise that the personalCap will be returned on `finalize()`.
	#promise?: TransactionArgument | undefined;
	// The personal kiosk argument.
	#personalCap?: TransactionObjectArgument;
	// A flag that checks whether kiosk TX is finalized.
	#finalized: boolean = false;

	constructor({
		transactionBlock,
		transaction = transactionBlock!,
		kioskClient,
		cap,
	}: KioskTransactionParams) {
		this.transaction = transaction;
		this.kioskClient = kioskClient;

		if (cap) this.setCap(cap);
	}

	/**
	 * Creates a kiosk and saves `kiosk` and `kioskOwnerCap` in state.
	 * Helpful if we want to chain some actions before sharing + transferring the cap to the specified address.
	 * @param borrow If true, the `kioskOwnerCap` is borrowed from the `PersonalKioskCap` to be used in next transactions.
	 */
	create() {
		this.#validateFinalizedStatus();
		this.#setPendingStatuses({
			share: true,
			transfer: true,
		});
		const [kiosk, cap] = kioskTx.createKiosk(this.transaction);
		this.kiosk = kiosk;
		this.kioskCap = cap;
		return this;
	}

	/**
	 * Creates a personal kiosk & shares it.
	 * The `PersonalKioskCap` is transferred to the signer.
	 * @param borrow If true, the `kioskOwnerCap` is borrowed from the `PersonalKioskCap` to be used in next transactions.
	 */
	createPersonal(borrow?: boolean) {
		this.#pendingShare = true;
		return this.create().convertToPersonal(borrow);
	}

	/**
	 * Converts a kiosk to a Personal (Soulbound) Kiosk.
	 * Requires initialization by either calling `ktxb.create()` or `ktxb.setCap()`.
	 */
	convertToPersonal(borrow?: boolean) {
		this.#validateKioskIsSet();

		const cap = convertToPersonalTx(
			this.transaction,
			this.kiosk!,
			this.kioskCap!,
			this.kioskClient.getRulePackageId('personalKioskRulePackageId'),
		);

		// if we enable `borrow`, we borrow the kioskCap from the cap.
		if (borrow) this.#borrowFromPersonalCap(cap);
		else this.#personalCap = cap;

		this.#setPendingStatuses({ transfer: true });
		return this;
	}

	/**
	 * Single function way to create a kiosk, share it and transfer the cap to the specified address.
	 */
	createAndShare(address: string) {
		this.#validateFinalizedStatus();
		const cap = kioskTx.createKioskAndShare(this.transaction);
		this.transaction.transferObjects([cap], this.transaction.pure.address(address));
	}

	/**
	 * Shares the kiosk.
	 */
	share() {
		this.#validateKioskIsSet();
		this.#setPendingStatuses({ share: false });
		kioskTx.shareKiosk(this.transaction, this.kiosk!);
	}

	/**
	 * Should be called only after `create` is called.
	 * It shares the kiosk & transfers the cap to the specified address.
	 */
	shareAndTransferCap(address: string) {
		if (this.#personalCap)
			throw new Error('You can only call `shareAndTransferCap` on a non-personal kiosk.');
		this.#setPendingStatuses({ transfer: false });
		this.share();
		this.transaction.transferObjects([this.kioskCap!], this.transaction.pure.address(address));
	}

	/**
	 * A function to borrow an item from a kiosk & execute any function with it.
	 * Example: You could borrow a Fren out of a kiosk, attach an accessory (or mix), and return it.
	 */
	borrowTx({ itemType, itemId }: ItemId, callback: (item: TransactionArgument) => void) {
		this.#validateKioskIsSet();
		const [itemObj, promise] = kioskTx.borrowValue(
			this.transaction,
			itemType,
			this.kiosk!,
			this.kioskCap!,
			itemId,
		);

		callback(itemObj);

		this.return({ itemType, item: itemObj, promise });
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
			this.transaction,
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
		kioskTx.returnValue(this.transaction, itemType, this.kiosk!, item, promise);
		return this;
	}

	/**
	 * A function to withdraw from kiosk
	 * @param address Where to trasnfer the coin.
	 * @param amount The amount we aim to withdraw.
	 */
	withdraw(address: string, amount?: string | bigint | number) {
		this.#validateKioskIsSet();
		const coin = kioskTx.withdrawFromKiosk(this.transaction, this.kiosk!, this.kioskCap!, amount);
		this.transaction.transferObjects([coin], this.transaction.pure.address(address));
		return this;
	}

	/**
	 * A function to place an item in the kiosk.
	 * @param itemType The type `T` of the item
	 * @param item The ID or Transaction Argument of the item
	 */
	place({ itemType, item }: ItemReference) {
		this.#validateKioskIsSet();
		kioskTx.place(this.transaction, itemType, this.kiosk!, this.kioskCap!, item);
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
		kioskTx.placeAndList(this.transaction, itemType, this.kiosk!, this.kioskCap!, item, price);
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
		kioskTx.list(this.transaction, itemType, this.kiosk!, this.kioskCap!, itemId, price);
		return this;
	}

	/**
	 * A function to delist an item from the kiosk.
	 * @param itemType The type `T` of the item
	 * @param itemId The ID of the item
	 */
	delist({ itemType, itemId }: ItemId) {
		this.#validateKioskIsSet();
		kioskTx.delist(this.transaction, itemType, this.kiosk!, this.kioskCap!, itemId);
		return this;
	}

	/**
	 * A function to take an item from the kiosk. The transaction won't succeed if the item is listed or locked.

	 * @param itemType The type `T` of the item
	 * @param itemId The ID of the item
	 */
	take({ itemType, itemId }: ItemId): TransactionObjectArgument {
		this.#validateKioskIsSet();
		return kioskTx.take(this.transaction, itemType, this.kiosk!, this.kioskCap!, itemId);
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
		this.transaction.transferObjects([item], this.transaction.pure.address(address));
		return this;
	}

	/**
	 * A function to take lock an item in the kiosk.

	 * @param itemType The type `T` of the item
	 * @param item The ID or Transaction Argument of the item
	 * @param itemId The ID of the item - Deprecated: Use `item` instead.
	 * @param policy The Policy ID or Transaction Argument for item T
	 */
	lock({
		itemType,
		item,
		itemId,
		policy,
	}: ItemReference & { policy: ObjectArgument; itemId?: string }) {
		this.#validateKioskIsSet();
		kioskTx.lock(this.transaction, itemType, this.kiosk!, this.kioskCap!, policy, itemId ?? item);
		return this;
	}

	/**
	 * Purchase an item from a seller's kiosk.
	 * Returns [item, transferRequest]
	 * Can be called like: `const [item, transferRequest] = kioskTx.purchase({...})`
	 * @param itemType The type `T` of the item
	 * @param itemId The ID of the item
	 * @param price The price in MIST
	 * @param sellerKiosk The kiosk which is selling the item. Can be an id or an object argument.
	 */
	purchase({
		itemType,
		itemId,
		price,
		sellerKiosk,
	}: ItemId & Price & { sellerKiosk: ObjectArgument }): [
		TransactionObjectArgument,
		TransactionObjectArgument,
	] {
		// Split the coin for the amount of the listing.
		const coin = this.transaction.splitCoins(this.transaction.gas, [
			this.transaction.pure.u64(price),
		]);
		return kioskTx.purchase(this.transaction, itemType, sellerKiosk, itemId, coin);
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

		// initialize the purchase `kiosk::purchase`
		const [purchasedItem, transferRequest] = this.purchase({
			itemType,
			itemId,
			price,
			sellerKiosk,
		});

		let canTransferOutsideKiosk = true;

		for (const rule of policy.rules) {
			const ruleDefinition = this.kioskClient.rules.find(
				(x) => getNormalizedRuleType(x.rule) === getNormalizedRuleType(rule),
			);
			if (!ruleDefinition) throw new Error(`No resolver for the following rule: ${rule}.`);

			if (ruleDefinition.hasLockingRule) canTransferOutsideKiosk = false;

			ruleDefinition.resolveRuleFunction({
				packageId: ruleDefinition.packageId,
				transactionBlock: this.transaction,
				transaction: this.transaction,
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

		confirmRequest(this.transaction, itemType, policy.id, transferRequest);

		if (canTransferOutsideKiosk) this.place({ itemType, item: purchasedItem });

		return this;
	}

	/**
	 * A function to setup the client using an existing `ownerCap`,
	 * as return from the `kioskClient.getOwnedKiosks` function.
	 * @param cap `KioskOwnerCap` object as returned from `getOwnedKiosks` SDK call.
	 */
	setCap(cap: KioskOwnerCap) {
		this.#validateFinalizedStatus();
		this.kiosk = this.transaction.object(cap.kioskId);
		if (!cap.isPersonal) {
			this.kioskCap = this.transaction.object(cap.objectId);
			return;
		}

		return this.#borrowFromPersonalCap(cap.objectId);
	}

	/**
	 *	A function that ends up the kiosk building tx & returns the `kioskOwnerCap` back to the
	 *  `PersonalKioskCap`, in case we are operating on a personal kiosk.
	 * 	It will also share the `kiosk` if it's not shared, and finalize the transfer of the personal cap if it's pending.
	 */
	finalize() {
		this.#validateKioskIsSet();
		// If we're pending the sharing of the new kiosk, share it.
		if (this.#pendingShare) this.share();

		// If we're operating on a non-personal kiosk, we don't need to do anything else.
		if (!this.#personalCap) {
			// If we're pending transfer though, we inform user to call `shareAndTransferCap()`.
			if (this.#pendingTransfer)
				throw new Error(
					'You need to transfer the `kioskOwnerCap` by calling `shareAndTransferCap()` before wrap',
				);
			return;
		}

		const packageId = this.kioskClient.getRulePackageId('personalKioskRulePackageId');

		// if we have a promise, return the `ownerCap` back to the personal cap.
		if (this.#promise) {
			this.transaction.moveCall({
				target: `${packageId}::personal_kiosk::return_val`,
				arguments: [this.#personalCap, this.transaction.object(this.kioskCap!), this.#promise!],
			});
		}

		// If we are pending transferring the personalCap, we do it here.
		if (this.#pendingTransfer)
			transferPersonalCapTx(this.transaction, this.#personalCap, packageId);

		// Mark the transaction as finalized, so no other functions can be called.
		this.#finalized = true;
	}

	// Some setters in case we want custom behavior.
	setKioskCap(cap: TransactionObjectArgument) {
		this.#validateFinalizedStatus();
		this.kioskCap = cap;
		return this;
	}

	setKiosk(kiosk: TransactionObjectArgument) {
		this.#validateFinalizedStatus();
		this.kiosk = kiosk;
		return this;
	}

	// Some getters
	/*
	 * Returns the active transaction's kiosk, or undefined if `setCap` or `create()` hasn't been called yet.
	 */
	getKiosk() {
		this.#validateFinalizedStatus();
		if (!this.kiosk) throw new Error('Kiosk is not set.');
		return this.kiosk;
	}

	/*
	 * Returns the active transaction's kioskOwnerCap, or undefined if `setCap` or `create()` hasn't been called yet.
	 */
	getKioskCap() {
		this.#validateFinalizedStatus();
		if (!this.kioskCap) throw new Error('Kiosk cap is not set');
		return this.kioskCap;
	}

	/**
	 * A function to borrow from `personalCap`.
	 */
	#borrowFromPersonalCap(personalCap: ObjectArgument) {
		const [kioskCap, promise] = this.transaction.moveCall({
			target: `${this.kioskClient.getRulePackageId(
				'personalKioskRulePackageId',
			)}::personal_kiosk::borrow_val`,
			arguments: [this.transaction.object(personalCap)],
		});

		this.kioskCap = kioskCap;
		this.#personalCap = this.transaction.object(personalCap);
		this.#promise = promise;

		return this;
	}

	#setPendingStatuses({ share, transfer }: { share?: boolean; transfer?: boolean }) {
		if (transfer !== undefined) this.#pendingTransfer = transfer;
		if (share !== undefined) this.#pendingShare = share;
	}

	#validateKioskIsSet() {
		this.#validateFinalizedStatus();

		if (!this.kiosk || !this.kioskCap)
			throw new Error(
				'You need to initialize the client by either supplying an existing owner cap or by creating a new by calling `.create()`',
			);
	}

	// Validates that `finalize`
	#validateFinalizedStatus() {
		if (this.#finalized)
			throw new Error("You can't add more transactions to a finalized kiosk transaction.");
	}
}
