// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import { TransactionBlock, TransactionArgument } from '@mysten/sui.js/transactions';

import { objArg } from '../utils';
import { KIOSK_MODULE, KIOSK_TYPE, ObjectArgument } from '../types';

/**
 * Create a new shared Kiosk and returns the [kiosk, kioskOwnerCap] tuple.
 */
export function createKiosk(tx: TransactionBlock): [TransactionArgument, TransactionArgument] {
	const [kiosk, kioskOwnerCap] = tx.moveCall({
		target: `${KIOSK_MODULE}::new`,
	});

	return [kiosk, kioskOwnerCap];
}

/**
 * Calls the `kiosk::new()` function and shares the kiosk.
 * Returns the `kioskOwnerCap` object.
 */
export function createKioskAndShare(tx: TransactionBlock): TransactionArgument {
	const [kiosk, kioskOwnerCap] = createKiosk(tx);
	shareKiosk(tx, kiosk);
	return kioskOwnerCap;
}

/**
 * Converts Transfer Policy to a shared object.
 */
export function shareKiosk(tx: TransactionBlock, kiosk: TransactionArgument) {
	tx.moveCall({
		target: `0x2::transfer::public_share_object`,
		typeArguments: [KIOSK_TYPE],
		arguments: [kiosk],
	});
}

/**
 * Call the `kiosk::place<T>(Kiosk, KioskOwnerCap, Item)` function.
 * Place an item to the Kiosk.
 */
export function place(
	tx: TransactionBlock,
	itemType: string,
	kiosk: ObjectArgument,
	kioskCap: ObjectArgument,
	item: ObjectArgument,
): void {
	tx.moveCall({
		target: `${KIOSK_MODULE}::place`,
		typeArguments: [itemType],
		arguments: [objArg(tx, kiosk), objArg(tx, kioskCap), objArg(tx, item)],
	});
}

/**
 * Call the `kiosk::lock<T>(Kiosk, KioskOwnerCap, TransferPolicy, Item)`
 * function. Lock an item in the Kiosk.
 *
 * Unlike `place` this function requires a `TransferPolicy` to exist
 * and be passed in. This is done to make sure the item does not get
 * locked without an option to take it out.
 */
export function lock(
	tx: TransactionBlock,
	itemType: string,
	kiosk: ObjectArgument,
	kioskCap: ObjectArgument,
	policy: ObjectArgument,
	item: ObjectArgument,
): void {
	tx.moveCall({
		target: `${KIOSK_MODULE}::lock`,
		typeArguments: [itemType],
		arguments: [objArg(tx, kiosk), objArg(tx, kioskCap), objArg(tx, policy), objArg(tx, item)],
	});
}

/**
 * Call the `kiosk::take<T>(Kiosk, KioskOwnerCap, ID)` function.
 * Take an item from the Kiosk.
 */
export function take(
	tx: TransactionBlock,
	itemType: string,
	kiosk: ObjectArgument,
	kioskCap: ObjectArgument,
	itemId: string,
): TransactionArgument {
	const [item] = tx.moveCall({
		target: `${KIOSK_MODULE}::take`,
		typeArguments: [itemType],
		arguments: [objArg(tx, kiosk), objArg(tx, kioskCap), tx.pure(itemId, 'address')],
	});

	return item;
}

/**
 * Call the `kiosk::list<T>(Kiosk, KioskOwnerCap, ID, u64)` function.
 * List an item for sale.
 */
export function list(
	tx: TransactionBlock,
	itemType: string,
	kiosk: ObjectArgument,
	kioskCap: ObjectArgument,
	itemId: string,
	price: string | bigint,
): void {
	tx.moveCall({
		target: `${KIOSK_MODULE}::list`,
		typeArguments: [itemType],
		arguments: [
			objArg(tx, kiosk),
			objArg(tx, kioskCap),
			tx.pure(itemId, 'address'),
			tx.pure(price, 'u64'),
		],
	});
}

/**
 * Call the `kiosk::list<T>(Kiosk, KioskOwnerCap, ID, u64)` function.
 * List an item for sale.
 */
export function delist(
	tx: TransactionBlock,
	itemType: string,
	kiosk: ObjectArgument,
	kioskCap: ObjectArgument,
	itemId: string,
): void {
	tx.moveCall({
		target: `${KIOSK_MODULE}::delist`,
		typeArguments: [itemType],
		arguments: [objArg(tx, kiosk), objArg(tx, kioskCap), tx.pure(itemId, 'address')],
	});
}

/**
 * Call the `kiosk::place_and_list<T>(Kiosk, KioskOwnerCap, Item, u64)` function.
 * Place an item to the Kiosk and list it for sale.
 */
export function placeAndList(
	tx: TransactionBlock,
	itemType: string,
	kiosk: ObjectArgument,
	kioskCap: ObjectArgument,
	item: ObjectArgument,
	price: string | bigint,
): void {
	tx.moveCall({
		target: `${KIOSK_MODULE}::place_and_list`,
		typeArguments: [itemType],
		arguments: [objArg(tx, kiosk), objArg(tx, kioskCap), objArg(tx, item), tx.pure(price, 'u64')],
	});
}

/**
 * Call the `kiosk::purchase<T>(Kiosk, ID, Coin<SUI>)` function and receive an Item and
 * a TransferRequest which needs to be dealt with (via a matching TransferPolicy).
 */
export function purchase(
	tx: TransactionBlock,
	itemType: string,
	kiosk: ObjectArgument,
	itemId: string,
	payment: ObjectArgument,
): [TransactionArgument, TransactionArgument] {
	const [item, transferRequest] = tx.moveCall({
		target: `${KIOSK_MODULE}::purchase`,
		typeArguments: [itemType],
		arguments: [objArg(tx, kiosk), tx.pure(itemId, 'address'), objArg(tx, payment)],
	});

	return [item, transferRequest];
}

/**
 * Call the `kiosk::withdraw(Kiosk, KioskOwnerCap, Option<u64>)` function and receive a Coin<SUI>.
 * If the amount is null, then the entire balance will be withdrawn.
 */
export function withdrawFromKiosk(
	tx: TransactionBlock,
	kiosk: ObjectArgument,
	kioskCap: ObjectArgument,
	amount?: string | bigint | number,
): TransactionArgument {
	const amountArg = tx.pure(amount ? { Some: amount } : { None: true }, 'Option<u64>');

	const [coin] = tx.moveCall({
		target: `${KIOSK_MODULE}::withdraw`,
		arguments: [objArg(tx, kiosk), objArg(tx, kioskCap), amountArg],
	});

	return coin;
}

/**
 * Call the `kiosk::borrow_value<T>(Kiosk, KioskOwnerCap, ID): T` function.
 * Immutably borrow an item from the Kiosk and return it in the end.
 *
 * Requires calling `returnValue` to return the item.
 */
export function borrowValue(
	tx: TransactionBlock,
	itemType: string,
	kiosk: ObjectArgument,
	kioskCap: ObjectArgument,
	itemId: string,
): [TransactionArgument, TransactionArgument] {
	const [item, promise] = tx.moveCall({
		target: `${KIOSK_MODULE}::borrow_val`,
		typeArguments: [itemType],
		arguments: [objArg(tx, kiosk), objArg(tx, kioskCap), tx.pure(itemId, 'address')],
	});

	return [item, promise];
}

/**
 * Call the `kiosk::return_value<T>(Kiosk, Item, Borrow)` function.
 * Return an item to the Kiosk after it was `borrowValue`-d.
 */
export function returnValue(
	tx: TransactionBlock,
	itemType: string,
	kiosk: ObjectArgument,
	item: TransactionArgument,
	promise: TransactionArgument,
): void {
	tx.moveCall({
		target: `${KIOSK_MODULE}::return_val`,
		typeArguments: [itemType],
		arguments: [objArg(tx, kiosk), item, promise],
	});
}
