// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import {
    normalizeSuiAddress,
    type SuiAddress,
    type SuiTransactionResponse,
} from '@mysten/sui.js';
import { useQuery, type UseQueryResult } from '@tanstack/react-query';

import { api } from '_redux/store/thunk-extras';

// Remove duplicate transactionsId, reduces the number of RPC calls
const dedupe = (results: string[] | undefined) =>
    results
        ? results.filter((value, index, self) => self.indexOf(value) === index)
        : [];

async function getTransactionsByAddress(
    address: string,
    cursor?: string
): Promise<SuiTransactionResponse[]> {
    const rpc = api.instance.fullNode;
    const txnsIds = await api.instance.fullNode.getTransactions(
        {
            FromAddress: address,
        },
        cursor
    );
    return rpc.getTransactionWithEffectsBatch(dedupe(txnsIds.data));
}

export function useGetTransactionsByAddress(
    address: SuiAddress
): UseQueryResult<SuiTransactionResponse[], unknown> {
    const normalizedAddress = normalizeSuiAddress(address);
    return useQuery(
        ['transactions-by-address', normalizedAddress],
        () => getTransactionsByAddress(normalizedAddress),
        {
            enabled: !!address,
            refetchOnWindowFocus: true,
        }
    );
}
