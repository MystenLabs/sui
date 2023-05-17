// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0
import {
    DryRunTransactionBlockResponse,
    type SuiAddress,
    type SuiTransactionBlockResponse,
    is,
    getExecutionStatusType,
    getTransactionDigest,
    getTransactionSender,
    SuiObjectChange,
} from '@mysten/sui.js';
import { useMemo } from 'react';

import { getBalanceChangeSummary } from '../utils/transaction/getBalanceChangeSummary';
import {
    WithDisplayFields,
    getObjectChangeSummary,
} from '../utils/transaction/getObjectChangeSummary';
import { getLabel } from '../utils/transaction/getLabel';
import { getGasSummary } from '../utils/transaction/getGasSummary';
import { useMultiGetObjectsDisplay } from './useMultiGetObjects';

function useGetObjectChangeDisplayFields(
    objectChanges: SuiObjectChange[] = []
) {
    const objectIds = objectChanges
        .map((change) => ('objectId' in change ? change.objectId : undefined))
        .filter(Boolean) as string[];
    const { data: objectDisplayData } = useMultiGetObjectsDisplay(objectIds);
    const changes = useMemo(
        () =>
            objectChanges.map((change) => ({
                ...change,
                display:
                    'objectId' in change
                        ? objectDisplayData?.get(change.objectId)
                        : null,
            })),
        [objectChanges, objectDisplayData]
    ) as WithDisplayFields<SuiObjectChange>[];

    return changes;
}

export function useTransactionSummary({
    transaction,
    currentAddress,
}: {
    transaction?: SuiTransactionBlockResponse | DryRunTransactionBlockResponse;
    currentAddress?: SuiAddress;
}) {
    const objectChanges = useGetObjectChangeDisplayFields(
        transaction?.objectChanges
    );
    const summary = useMemo(() => {
        if (!transaction) return null;
        const objectSummary = getObjectChangeSummary(
            objectChanges,
            currentAddress
        );
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
    }, [transaction, currentAddress, objectChanges]);

    return summary;
}
