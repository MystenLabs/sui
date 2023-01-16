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
    address: string
): Promise<SuiTransactionResponse[]> {
    const rpc = api.instance.fullNode;
    const normalizedAddress = normalizeSuiAddress(address);
    const txnsIds = (await rpc.getTransactionsForAddress(
        normalizedAddress,
        true
    )) as string[];
    return rpc.getTransactionWithEffectsBatch(dedupe(txnsIds));
}

export function useGetTranactionByAddress(
    address: SuiAddress
): UseQueryResult<SuiTransactionResponse[], unknown> {
    const normalizedAddress = normalizeSuiAddress(address);
    return useQuery(
        ['transaction-by-address', normalizedAddress],
        () => getTransactionsByAddress(normalizedAddress),
        {
            enabled: !!address,
            refetchOnWindowFocus: true,
        }
    );
}
