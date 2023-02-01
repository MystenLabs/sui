// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import { getTotalGasUsed } from '@mysten/sui.js';
import { useMemo } from 'react';

import {
    useTransactionDryRun,
    type TransactionDryRun,
} from './useTransactionDryRun';
import { getEventsSummary } from '_helpers';

import type { TxnMetaResponse } from '../helpers/getEventsSummary';

type ExecuteDryRunTransactionRequestProps = {
    txData: TransactionDryRun;
    activeAddress: string;
};

type ExecuteDryRunTransactionReqResponse = [
    TxnMetaResponse | null,
    number | null
];

export function useTransactionSummary({
    txData,
    activeAddress,
}: ExecuteDryRunTransactionRequestProps): ExecuteDryRunTransactionReqResponse {
    console.log(txData);

    const { data } = useTransactionDryRun(txData);

    const eventsSummary = useMemo(
        () => (data ? getEventsSummary(data, activeAddress) : null),
        [data, activeAddress]
    );
    const txGasEstimation = data && getTotalGasUsed(data);

    return [eventsSummary, txGasEstimation || null];
}
