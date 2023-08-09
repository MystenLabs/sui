// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0
import {
	DryRunTransactionBlockResponse,
	type SuiTransactionBlockResponse,
} from '@mysten/sui.js/client';
import { useMemo } from 'react';

import { getBalanceChangeSummary } from '../utils/transaction/getBalanceChangeSummary';
import {
	SuiObjectChangeWithDisplay,
	getObjectChangeSummary,
} from '../utils/transaction/getObjectChangeSummary';
import { getLabel } from '../utils/transaction/getLabel';
import { getGasSummary } from '../utils/transaction/getGasSummary';
import { useMultiGetObjects } from './useMultiGetObjects';
import { getObjectDisplayLookup } from '../utils/transaction/getObjectDisplayLookup';

export function useTransactionSummary({
	transaction,
	currentAddress,
	recognizedPackagesList,
}: {
	transaction?: SuiTransactionBlockResponse | DryRunTransactionBlockResponse;
	currentAddress?: string;
	recognizedPackagesList: string[];
}) {
	const { objectChanges } = transaction ?? {};

	const objectIds = objectChanges
		?.map((change) => 'objectId' in change && change.objectId)
		.filter(Boolean) as string[];

	const { data } = useMultiGetObjects(objectIds, { showDisplay: true });
	const lookup = getObjectDisplayLookup(data);

	const objectChangesWithDisplay = useMemo(
		() =>
			[...(objectChanges ?? [])].map((change) => ({
				...change,
				display: 'objectId' in change ? lookup?.get(change.objectId) : null,
			})),
		[lookup, objectChanges],
	) as SuiObjectChangeWithDisplay[];

	const summary = useMemo(() => {
		if (!transaction) return null;
		const objectSummary = getObjectChangeSummary(objectChangesWithDisplay);
		const balanceChangeSummary = getBalanceChangeSummary(transaction, recognizedPackagesList);
		const gas = getGasSummary(transaction);

		if ('digest' in transaction) {
			// Non-dry-run transaction:
			return {
				gas,
				sender: transaction.transaction?.data.sender,
				balanceChanges: balanceChangeSummary,
				digest: transaction.digest,
				label: getLabel(transaction, currentAddress),
				objectSummary,
				status: transaction.effects?.status.status,
				timestamp: transaction.timestampMs,
			};
		} else {
			// Dry run transaction:
			return {
				gas,
				objectSummary,
				balanceChanges: balanceChangeSummary,
			};
		}
	}, [transaction, objectChangesWithDisplay, recognizedPackagesList, currentAddress]);

	return summary;
}
