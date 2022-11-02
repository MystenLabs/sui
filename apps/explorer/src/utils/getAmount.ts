// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import {
    getPaySuiTransaction,
    getPayTransaction,
    getTransferSuiAmount,
} from '@mysten/sui.js';

import type { SuiTransactionKind } from '@mysten/sui.js';

// TODO: Move this to sui.js
export function getAmount(txnData: SuiTransactionKind): number | bigint | null {
    const paySuiData =
        getPaySuiTransaction(txnData) ?? getPayTransaction(txnData);
    // Sum Sui Pay Array Amounts
    const paySuiAmount = paySuiData?.amounts.reduce(
        (acc, value) => value + acc,
        0
    );
    return paySuiAmount ?? getTransferSuiAmount(txnData);
}
