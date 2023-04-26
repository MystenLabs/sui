// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import {
  SharedObjectRef,
  SuiObjectRef,
  TransactionArgument,
  TransactionBlock,
} from '@mysten/sui.js';

/**
 * A valid argument for any of the Kiosk functions.
 */
export type ObjectArgument =
  | string
  | TransactionArgument
  | SharedObjectRef
  | SuiObjectRef;

/**
 * Convert any valid input into a TransactionArgument.
 *
 * @param tx The transaction to use for creating the argument.
 * @param arg The argument to convert.
 * @returns The converted TransactionArgument.
 */
export function objArg(
  tx: TransactionBlock,
  arg: string | SharedObjectRef | SuiObjectRef | TransactionArgument,
): TransactionArgument {
  if (typeof arg === 'string') {
    return tx.object(arg);
  }

  if ('digest' in arg && 'version' in arg && 'objectId' in arg) {
    return tx.objectRef(arg);
  }

  if ('objectId' in arg && 'initialSharedVersion' in arg && 'mutable' in arg) {
    return tx.sharedObjectRef(arg);
  }

  if ('kind' in arg) {
    return arg;
  }

  throw new Error('Invalid argument type');
}
