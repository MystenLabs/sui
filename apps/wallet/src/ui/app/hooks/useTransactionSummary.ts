// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import { getTotalGasUsed, getGasData, type SuiGasData } from '@mysten/sui.js';
import { useMemo } from 'react';

import {
    useTransactionDryRun,
    type TransactionDryRun,
} from './useTransactionDryRun';
import { getEventsSummary } from '_helpers';

import type { TxnMetaResponse } from '../helpers/getEventsSummary';

type ExecuteDryRunTransactionRequestProps = {
    txData: TransactionDryRun;
    addressForTransaction: string;
};

type ExecuteDryRunTransactionReqResponse = [
    TxnMetaResponse | null,
    number | null,
    SuiGasData | null
];

export function useTransactionSummary({
    txData,
    addressForTransaction,
}: ExecuteDryRunTransactionRequestProps): ExecuteDryRunTransactionReqResponse {
    const { data } = useTransactionDryRun(txData, addressForTransaction);

    const eventsSummary = useMemo(
        () =>
            data ? getEventsSummary(data.events, addressForTransaction) : null,
        [data, addressForTransaction]
    );
    const txGasEstimation = data ? getTotalGasUsed(data.effects) : null;
    const txGasData = data ? getGasData(data) : null;

    return [eventsSummary, txGasEstimation ?? null, txGasData];
}
