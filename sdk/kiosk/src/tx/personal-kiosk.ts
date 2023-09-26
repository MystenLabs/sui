// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import { TransactionArgument, TransactionBlock } from '@mysten/sui.js/transactions';

import { ObjectArgument } from '../types';
import { objArg } from '../utils';

export function convertToPersonalTx(
	tx: TransactionBlock,
	kiosk: ObjectArgument,
	kioskOwnerCap: ObjectArgument,
	packageId: string,
): TransactionArgument {
	const personalKioskCap = tx.moveCall({
		target: `${packageId}::personal_kiosk::new`,
		arguments: [objArg(tx, kiosk), objArg(tx, kioskOwnerCap)],
	});

	return personalKioskCap;
}

/**
 * Transfers the personal kiosk Cap to the sender.
 */
export function transferPersonalCapTx(
	tx: TransactionBlock,
	personalKioskCap: TransactionArgument,
	packageId: string,
) {
	tx.moveCall({
		target: `${packageId}::personal_kiosk::transfer_to_sender`,
		arguments: [personalKioskCap],
	});
}
