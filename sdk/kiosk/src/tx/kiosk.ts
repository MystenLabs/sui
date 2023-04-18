// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import {
  SuiAddress,
  TransactionArgument,
  TransactionBlock,
} from '@mysten/sui.js';

import { ObjectArgument, objArg } from '../utils';
import { TransferRequest } from './transfer-request';

/** The Kiosk module. */
export const KIOSK_MODULE = '0x2::kiosk';

/** The Kiosk type. */
export const KIOSK_TYPE = `${KIOSK_MODULE}::Kiosk`;

/**
 * Create a new shared Kiosk and return the `KioskOwnerCap`
 * and the transaction to continue building.
 */
export function createKiosk(
  tx: TransactionBlock,
): [TransactionArgument, TransactionArgument] {
  let [kiosk, kioskOwnerCap] = tx.moveCall({
    target: `${KIOSK_MODULE}::new`,
  });

  return [kiosk, kioskOwnerCap];
}

export function createKioskAndShare(
  tx: TransactionBlock,
): TransactionArgument {
  let [kiosk, kioskOwnerCap] = tx.moveCall({
    target: `${KIOSK_MODULE}::new`,
  });

  tx.moveCall({
    target: `0x2::transfer::public_share_object`,
    arguments: [kiosk],
    typeArguments: [KIOSK_TYPE],
  });

  return kioskOwnerCap;
}

/**
 * Call the `kiosk::place<T>(Kiosk, KioskOwnerCap, Item)` function.
 * Place an item to the Kiosk.
 */
export function place(
  tx: TransactionBlock,
  kiosk: ObjectArgument,
  kioskCap: ObjectArgument,
  item: ObjectArgument,
  itemType: string,
) {
  tx.moveCall({
    target: `${KIOSK_MODULE}::place`,
    typeArguments: [itemType],
    arguments: [objArg(tx, kiosk), objArg(tx, kioskCap), objArg(tx, item)],
  });

  return tx;
}

/**
 * Call the `kiosk::take<T>(Kiosk, KioskOwnerCap, ID)` function.
 * Take an item from the Kiosk.
 */
export function take(
  tx: TransactionBlock,
  kiosk: ObjectArgument,
  kioskCap: ObjectArgument,
  itemId: SuiAddress,
  itemType: string,
): TransactionArgument {
  let [item] = tx.moveCall({
    target: `${KIOSK_MODULE}::take`,
    typeArguments: [itemType],
    arguments: [
      objArg(tx, kiosk),
      objArg(tx, kioskCap),
      tx.pure(itemId, 'address'),
    ],
  });

  return item;
}

/**
 * Call the `kiosk::list<T>(Kiosk, KioskOwnerCap, ID, u64)` function.
 * List an item for sale.
 */
export function list(
  tx: TransactionBlock,
  kiosk: ObjectArgument,
  kioskCap: ObjectArgument,
  itemId: SuiAddress,
  price: string | bigint,
  itemType: string,
) {
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

  return tx;
}

/**
 * Call the `kiosk::place_and_list<T>(Kiosk, KioskOwnerCap, Item, u64)` function.
 * Place an item to the Kiosk and list it for sale.
 */
export function placeAndList(
  tx: TransactionBlock,
  kiosk: ObjectArgument,
  kioskCap: ObjectArgument,
  item: ObjectArgument,
  itemType: string,
  price: string | bigint,
) {
  tx.moveCall({
    target: `${KIOSK_MODULE}::place_and_list`,
    typeArguments: [itemType],
    arguments: [
      objArg(tx, kiosk),
      objArg(tx, kioskCap),
      objArg(tx, item),
      tx.pure(price, 'u64'),
    ],
  });

  return tx;
}

/**
 * Call the `kiosk::purchase<T>(Kiosk, ID, Coin<SUI>)` function and receive an Item and
 * a TransferRequest which needs to be dealt with (via a matching TransferPolicy).
 */
export function purchase(
  tx: TransactionBlock,
  kiosk: ObjectArgument,
  itemId: SuiAddress,
  payment: ObjectArgument,
  itemType: string,
): [TransactionArgument, TransferRequest] {
  let [item, transferRequest] = tx.moveCall({
    target: `${KIOSK_MODULE}::purchase`,
    typeArguments: [itemType],
    arguments: [
      objArg(tx, kiosk),
      tx.pure(itemId, 'address'),
      objArg(tx, payment),
    ],
  });

  return [item, { ...transferRequest, itemType } as TransferRequest];
}

/**
 * Call the `kiosk::withdraw(Kiosk, KioskOwnerCap, Option<u64>)` function and receive a Coin<SUI>.
 * If the amount is null, then the entire balance will be withdrawn.
 */
export function withdrawFromKiosk(
  kiosk: ObjectArgument,
  kioskCap: ObjectArgument,
  amount: string | bigint | null,
  tx: TransactionBlock,
): TransactionArgument {
  let amountArg =
    amount !== null
      ? tx.pure(amount, 'vector<u64>')
      : tx.pure([], 'vector<u64>');

  let [coin] = tx.moveCall({
    target: `${KIOSK_MODULE}::withdraw`,
    arguments: [objArg(tx, kiosk), objArg(tx, kioskCap), amountArg],
  });

  return coin;
}

/**
 * Call the `kiosk::borrow<T>(Kiosk, KioskOwnerCap, ID): &T` function.
 * Immutably borrow an item from the Kiosk.
 */
export function borrow(
  kiosk: ObjectArgument,
  kioskCap: ObjectArgument,
  itemId: SuiAddress,
  itemType: string,
  tx: TransactionBlock,
): TransactionArgument {
  let [item] = tx.moveCall({
    target: `${KIOSK_MODULE}::borrow`,
    typeArguments: [itemType],
    arguments: [
      objArg(tx, kiosk),
      objArg(tx, kioskCap),
      tx.pure(itemId, 'address'),
    ],
  });

  return item;
}

/**
 * Call the `kiosk::borrow_mut<T>(Kiosk, KioskOwnerCap, ID): &mut T` function.
 * Mutably borrow an item from the Kiosk.
 */
export function borrowMut(
  kiosk: ObjectArgument,
  kioskCap: ObjectArgument,
  itemId: SuiAddress,
  itemType: string,
  tx: TransactionBlock,
): TransactionArgument {
  let [item] = tx.moveCall({
    target: `${KIOSK_MODULE}::borrow_mut`,
    typeArguments: [itemType],
    arguments: [
      objArg(tx, kiosk),
      objArg(tx, kioskCap),
      tx.pure(itemId, 'address'),
    ],
  });

  return item;
}

/**
 * Call the `kiosk::borrow_value<T>(Kiosk, KioskOwnerCap, ID): T` function.
 * Immutably borrow an item from the Kiosk and return it in the end.
 *
 * Requires calling `returnValue` to return the item.
 */
export function borrowValue(
  kiosk: ObjectArgument,
  kioskCap: ObjectArgument,
  itemId: SuiAddress,
  itemType: string,
  tx: TransactionBlock,
): [TransactionArgument, TransactionArgument] {
  let [item, promise] = tx.moveCall({
    target: `${KIOSK_MODULE}::borrow_value`,
    typeArguments: [itemType],
    arguments: [
      objArg(tx, kiosk),
      objArg(tx, kioskCap),
      tx.pure(itemId, 'address'),
    ],
  });

  return [item, promise];
}

/**
 * Call the `kiosk::return_value<T>(Kiosk, Item, Borrow)` function.
 * Return an item to the Kiosk after it was `borrowValue`-d.
 */
export function returnValue(
  kiosk: ObjectArgument,
  item: TransactionArgument,
  promise: TransactionArgument,
  itemType: string,
  tx: TransactionBlock,
) {
  tx.moveCall({
    target: `${KIOSK_MODULE}::return_value`,
    typeArguments: [itemType],
    arguments: [objArg(tx, kiosk), item, promise],
  });

  return tx;
}
