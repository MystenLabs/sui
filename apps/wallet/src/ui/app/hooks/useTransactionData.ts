// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import { useFormatCoin, useRpcClient } from '@mysten/core';
import { type SuiAddress, SUI_TYPE_ARG, Transaction } from '@mysten/sui.js';
import { useQuery } from '@tanstack/react-query';
import { useMemo } from 'react';

export function useTransactionData(
    sender?: SuiAddress | null,
    transaction?: Transaction | null
) {
    const rpc = useRpcClient();
    const clonedTransaction = useMemo(() => {
        if (!transaction) return;

        const tx = new Transaction(transaction);
        if (sender) {
            tx.setSenderIfNotSet(sender);
        }
        return tx;
    }, [transaction, sender]);

    return useQuery(
        ['transaction-data', clonedTransaction?.serialize()],
        async () => {
            // Build the transaction to bytes, which will ensure that the transaction data is fully populated:
            await clonedTransaction!.build({ provider: rpc });
            return clonedTransaction!.transactionData;
        },
        {
            enabled: !!clonedTransaction,
        }
    );
}

export function useTransactionGasBudget(
    sender?: SuiAddress | null,
    transaction?: Transaction | null
) {
    const { data, ...rest } = useTransactionData(sender, transaction);

    const [formattedGas] = useFormatCoin(data?.gasConfig.budget, SUI_TYPE_ARG);

    return {
        data: formattedGas,
        ...rest,
    };
}
