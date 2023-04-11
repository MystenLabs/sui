// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import { useFormatCoin, useRpcClient } from '@mysten/core';
import {
    type SuiAddress,
    SUI_TYPE_ARG,
    TransactionBlock,
} from '@mysten/sui.js';
import { useQuery } from '@tanstack/react-query';

export function useTransactionData(
    sender?: SuiAddress | null,
    transaction?: TransactionBlock | null
) {
    const rpc = useRpcClient();
    return useQuery(
        ['transaction-data', transaction?.serialize()],
        async () => {
            const clonedTransaction = new TransactionBlock(transaction!);
            if (sender) {
                clonedTransaction.setSenderIfNotSet(sender);
            }
            // Build the transaction to bytes, which will ensure that the transaction data is fully populated:
            await clonedTransaction!.build({ provider: rpc });
            return clonedTransaction!.blockData;
        },
        {
            enabled: !!transaction,
        }
    );
}

export function useTransactionGasBudget(
    sender?: SuiAddress | null,
    transaction?: TransactionBlock | null
) {
    const { data, ...rest } = useTransactionData(sender, transaction);

    const [formattedGas] = useFormatCoin(data?.gasConfig.budget, SUI_TYPE_ARG);

    return {
        data: formattedGas,
        ...rest,
    };
}
