// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0
import {
    DryRunTransactionBlockResponse,
    type SuiAddress,
    type SuiTransactionBlockResponse,
    is,
    getExecutionStatusType,
    getTransactionDigest,
} from '@mysten/sui.js';
import { useMemo } from 'react';

import { getBalanceChangeSummary } from '../utils/transaction/getBalanceChangeSummary';
import { getObjectChangeSummary } from '../utils/transaction/getObjectChangeSummary';
import { getLabel } from '../utils/transaction/getLabel';
import { getGasSummary } from '../utils/transaction/getGasSummary';

const getSummary = (
    transaction: DryRunTransactionBlockResponse | SuiTransactionBlockResponse,
    currentAddress?: SuiAddress
) => {
    const objectSummary = getObjectChangeSummary(transaction, currentAddress);
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
            balanceChanges: balanceChangeSummary,
            digest: getTransactionDigest(transaction),
            label: getLabel(transaction),
            objectSummary,
            status: getExecutionStatusType(transaction),
            timestamp: transaction.timestampMs,
        };
    }
};

export function useTransactionSummary({
    transaction,
    currentAddress,
}: {
    transaction?: SuiTransactionBlockResponse | DryRunTransactionBlockResponse;
    currentAddress?: SuiAddress;
}) {
    const summary = useMemo(() => {
        if (!transaction) return null;
        return getSummary(transaction, currentAddress);
    }, [transaction, currentAddress]);

    return summary;
}
