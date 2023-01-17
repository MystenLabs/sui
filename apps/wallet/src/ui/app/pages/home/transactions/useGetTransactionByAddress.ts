// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import {
    normalizeSuiAddress,
    type SuiAddress,
    type ObjectId,
    type SuiTransactionResponse,
} from '@mysten/sui.js';
import { useQuery, type UseQueryResult } from '@tanstack/react-query';

import { useAppSelector, useRpc } from '_hooks';

// Remove duplicate transactionsId, reduces the number of RPC calls
const dedupe = (results: string[] | undefined) =>
    results
        ? results.filter((value, index, self) => self.indexOf(value) === index)
        : [];

export function useGetTranactionIdByAddress(
    address: SuiAddress
): UseQueryResult<string[], unknown> {
    const network = useAppSelector((state) => state.app.apiEnv);
    const normalizedAddress = normalizeSuiAddress(address);
    const rpc = useRpc();
    return useQuery(
        ['transactions-by-address', normalizedAddress, network],
        async () => {
            const txnIds = await rpc.getTransactionsForAddress(
                normalizedAddress,
                true
            );
            return dedupe(txnIds);
        },
        {
            enabled: !!address,
            refetchOnWindowFocus: true,
        }
    );
}

export function useGetTransactionById(
    transactionId: ObjectId
): UseQueryResult<SuiTransactionResponse, unknown> {
    const network = useAppSelector((state) => state.app.apiEnv);
    const rpc = useRpc();
    return useQuery(
        ['transaction-by-id', transactionId, network],
        () => rpc.getTransactionWithEffects(transactionId),
        {
            enabled: !!transactionId,
        }
    );
}
