// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import {
    getTransactionKindName,
    getMoveCallTransaction,
    getTransactions,
} from '@mysten/sui.js';

import type { SuiTransactionResponse } from '@mysten/sui.js';

export function checkStakingTxn(txn: SuiTransactionResponse) {
    const [transaction] = getTransactions(txn);
    const txnKind = getTransactionKindName(transaction);

    if (txnKind !== 'Call') return null;

    const moveCallTxn = getMoveCallTransaction(transaction);
    if (
        moveCallTxn?.module === 'sui_system' &&
        moveCallTxn?.function === 'request_add_delegation_mul_coin'
    )
        return 'Staked';
    if (
        moveCallTxn?.module === 'sui_system' &&
        moveCallTxn?.function === 'request_withdraw_delegation'
    )
        return 'Unstaked';
    return null;
}
