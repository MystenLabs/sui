// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0
import {
	DryRunTransactionBlockResponse,
	type SuiTransactionBlockResponse,
	getExecutionStatusType,
	getTransactionDigest,
	getTransactionSender,
} from '@mysten/sui.js';
import { is } from '@mysten/sui.js/utils';
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
}: {
	transaction?: SuiTransactionBlockResponse | DryRunTransactionBlockResponse;
	currentAddress?: string;
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
		const balanceChangeSummary = getBalanceChangeSummary(transaction);
		const gas = getGasSummary(transaction);

		if (is(transaction, DryRunTransactionBlockResponse)) {
			return {
				gas,
				objectSummary,
				balanceChanges: balanceChangeSummary,
			};
		} else {
			return {
				gas,
				sender: getTransactionSender(transaction),
				balanceChanges: balanceChangeSummary,
				digest: getTransactionDigest(transaction),
				label: getLabel(transaction, currentAddress),
				objectSummary,
				status: getExecutionStatusType(transaction),
				timestamp: transaction.timestampMs,
			};
		}
	}, [transaction, currentAddress, objectChangesWithDisplay]);

	return summary;
}
