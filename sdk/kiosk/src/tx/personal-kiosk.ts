// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import type { TransactionBlock, TransactionObjectArgument } from '@mysten/sui.js/transactions';

import type { ObjectArgument } from '../types/index.js';

export function convertToPersonalTx(
	tx: TransactionBlock,
	kiosk: ObjectArgument,
	kioskOwnerCap: ObjectArgument,
	packageId: string,
): TransactionObjectArgument {
	const personalKioskCap = tx.moveCall({
		target: `${packageId}::personal_kiosk::new`,
		arguments: [tx.object(kiosk), tx.object(kioskOwnerCap)],
	});

	return personalKioskCap;
}

/**
 * Transfers the personal kiosk Cap to the sender.
 */
export function transferPersonalCapTx(
	tx: TransactionBlock,
	personalKioskCap: TransactionObjectArgument,
	packageId: string,
) {
	tx.moveCall({
		target: `${packageId}::personal_kiosk::transfer_to_sender`,
		arguments: [personalKioskCap],
	});
}
