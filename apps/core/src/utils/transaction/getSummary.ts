// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0
import {
    type SuiAddress,
    type DryRunTransactionBlockResponse,
    SuiTransactionBlockResponse,
    getTransactionSender,
    is,
} from '@mysten/sui.js';

import {
    type BalanceChangeSummary,
    getBalanceChangeSummary,
} from './getBalanceChangeSummary';
import {
    ObjectChangeSummary,
    getObjectChangeSummary,
} from './getObjectChangeSummary';
import { GasSummaryType, getGasSummary } from './getGasSummary';

export type TransactionSummary = {
    digest?: string;
    sender?: SuiAddress;
    timestamp?: string;
    balanceChanges: BalanceChangeSummary[] | null;
    gas?: GasSummaryType;
    objectSummary: ObjectChangeSummary | null;
} | null;

export const getTransactionSummary = (
    transaction: DryRunTransactionBlockResponse | SuiTransactionBlockResponse,
    currentAddress: SuiAddress
): TransactionSummary => {
    const { effects } = transaction;
    if (!effects) return null;

    const sender = is(transaction, SuiTransactionBlockResponse)
        ? getTransactionSender(transaction)
        : undefined;
    const gasSummary = getGasSummary(transaction);

    const balanceChangeSummary = getBalanceChangeSummary(transaction);
    const objectChangeSummary = getObjectChangeSummary(
        transaction,
        currentAddress
    );

    return {
        sender,
        balanceChanges: balanceChangeSummary,
        gas: gasSummary,
        objectSummary: objectChangeSummary,
    };
};
