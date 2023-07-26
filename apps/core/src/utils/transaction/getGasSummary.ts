// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0
import { getTotalGasUsed } from '@mysten/sui.js';
import {
	DryRunTransactionBlockResponse,
	GasCostSummary,
	SuiTransactionBlockResponse,
	SuiGasData,
} from '@mysten/sui.js/client';

type Optional<T> = {
	[K in keyof T]?: T[K];
};

export type GasSummaryType =
	| (GasCostSummary &
			Optional<SuiGasData> & {
				totalGas?: string;
				owner?: string;
				isSponsored: boolean;
				gasUsed: GasCostSummary;
			})
	| null;

export function getGasSummary(
	transaction: SuiTransactionBlockResponse | DryRunTransactionBlockResponse,
): GasSummaryType {
	const { effects } = transaction;
	if (!effects) return null;
	const totalGas = getTotalGasUsed(effects);

	let sender = 'transaction' in transaction ? transaction.transaction?.data.sender : undefined;

	const gasData = 'transaction' in transaction ? transaction.transaction?.data.gasData : {};

	const owner =
		'transaction' in transaction
			? transaction.transaction?.data.gasData.owner
			: typeof effects.gasObject.owner === 'object' && 'AddressOwner' in effects.gasObject.owner
			? effects.gasObject.owner.AddressOwner
			: '';

	return {
		...effects.gasUsed,
		...gasData,
		owner,
		totalGas: totalGas?.toString(),
		isSponsored: !!owner && !!sender && owner !== sender,
		gasUsed: transaction?.effects!.gasUsed,
	};
}
