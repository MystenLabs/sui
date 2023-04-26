// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import { TransactionArgument, TransactionBlock } from '@mysten/sui.js';
import { ObjectArgument, objArg } from '../utils';

/**
 * A Hot Potato struct that is returned in the `kiosk::purchase` function.
 * Can only be consumed by the `transfer_policy::confirm_request`.
 */
export type TransferRequest = {
  kind: 'NestedResult';
  index: number;
  resultIndex: number;
  itemType: string;
};

/**
 * Call the `transfer_policy::confirm_request` function to unblock the
 * transaction.
 */
export function confirmRequest(
  tx: TransactionBlock,
  policy: ObjectArgument,
  request: TransferRequest,
) {
  tx.moveCall({
    target: `0x2::transfer_policy::confirm_request`,
    typeArguments: [request.itemType],
    arguments: [objArg(tx, policy), request as TransactionArgument],
  });

  return null;
}
