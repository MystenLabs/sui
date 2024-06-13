// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import type { SuiEvent, SuiTransactionBlockKind, TransactionEffects } from '@mysten/sui/client';

type FormattedBalance = {
	amount?: number | null;
	coinType?: string | null;
	recipientAddress: string;
}[];

export function getAmount(
	_txnData: SuiTransactionBlockKind,
	_txnEffect: TransactionEffects,
	_events: SuiEvent[],
): FormattedBalance | null {
	// TODO: Support programmable transactions:
	return null;
}
