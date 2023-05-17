// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import { useTransactionSummaryWithNFTs } from '@mysten/core';
import {
    getTransactionKind,
    getTransactionKindName,
    type SuiTransactionBlockResponse,
} from '@mysten/sui.js';

import { BalanceChanges } from './BalanceChanges';
import { ObjectChanges } from './ObjectChanges';

import { LoadingSpinner } from '~/ui/LoadingSpinner';
import { Text } from '~/ui/Text';

interface TransactionSummaryProps {
    transaction: SuiTransactionBlockResponse;
}

export function TransactionSummary({ transaction }: TransactionSummaryProps) {
    const summaryData = useTransactionSummaryWithNFTs({
        transaction,
    });

    const transactionKindName = getTransactionKindName(
        getTransactionKind(transaction)!
    );

    const balanceChanges = summaryData?.balanceChanges;
    const objectSummary = summaryData?.objectSummary;
    const objectSummaryNFTData = summaryData?.objectSummaryNFTData;

    if (summaryData?.isLoading) {
        return (
            <div className="relative h-verticalListLong">
                <div className="absolute left-1/2 top-1/2 flex -translate-x-1/2 -translate-y-1/2 flex-col items-center gap-2">
                    <LoadingSpinner />
                    <Text variant="pBody/medium" color="steel-dark">
                        Loading
                    </Text>
                </div>
            </div>
        );
    }

    return (
        <div className="flex flex-wrap gap-4 md:gap-8">
            {balanceChanges &&
                transactionKindName === 'ProgrammableTransaction' && (
                    <BalanceChanges changes={balanceChanges} />
                )}
            {objectSummary && (
                <ObjectChanges
                    objectSummary={objectSummary}
                    objectSummaryNFTData={objectSummaryNFTData}
                />
            )}
        </div>
    );
}
