// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0
import {
	ObjectOwner,
	type DryRunTransactionBlockResponse,
	type SuiTransactionBlockResponse,
} from '@mysten/sui.js';

export type BalanceChange = {
	coinType: string;
	amount: string;
	recipient?: string;
	owner?: string;
};

export type BalanceChangeByOwner = Record<string, BalanceChange[]>;
export type BalanceChangeSummary = BalanceChangeByOwner | null;

function getOwnerAddress(owner: ObjectOwner): string {
	if (typeof owner === 'object') {
		if ('AddressOwner' in owner) {
			return owner.AddressOwner;
		} else if ('ObjectOwner' in owner) {
			return owner.ObjectOwner;
		} else if ('Shared' in owner) {
			return 'Shared';
		}
	}
	return '';
}

export const getBalanceChangeSummary = (
	transaction: DryRunTransactionBlockResponse | SuiTransactionBlockResponse,
) => {
	const { balanceChanges, effects } = transaction;
	if (!balanceChanges || !effects) return null;

	const balanceChangeByOwner = {};
	return balanceChanges.reduce((acc, balanceChange) => {
		const amount = BigInt(balanceChange.amount);
		const owner = getOwnerAddress(balanceChange.owner);

		const recipient = balanceChanges.find(
			(bc) => balanceChange.coinType === bc.coinType && amount === BigInt(bc.amount) * -1n,
		);

		const recipientAddress = recipient?.owner ? getOwnerAddress(recipient?.owner) : undefined;

		const summary = {
			coinType: balanceChange.coinType,
			amount: amount.toString(),
			recipient: recipientAddress,
			owner,
		};

		acc[owner] = (acc[owner] ?? []).concat(summary);
		return acc;
	}, balanceChangeByOwner as BalanceChangeByOwner);
};
