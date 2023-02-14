// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import {
    isValidTransactionDigest,
    isValidSuiAddress,
    isValidSuiObjectId,
    normalizeSuiObjectId,
    is,
    SuiObject,
    type JsonRpcProvider,
} from '@mysten/sui.js';
import { useQuery } from '@tanstack/react-query';

import { useRpc } from '~/hooks/useRpc';
import { isGenesisLibAddress } from '~/utils/api/searchUtil';

type Result = {
    label: string;
    results: { id: string; label: string; type: string }[];
};

const getResultsForTransaction = async (
    rpc: JsonRpcProvider,
    query: string
) => {
    if (!isValidTransactionDigest(query)) return null;

    const txdata = await rpc.getTransactionWithEffects(query);
    return {
        label: 'transaction',
        results: [
            {
                id: txdata.certificate.transactionDigest,
                label: txdata.certificate.transactionDigest,
                type: 'transaction',
            },
        ],
    };
};

const getResultsForObject = async (rpc: JsonRpcProvider, query: string) => {
    const normalized = normalizeSuiObjectId(query);
    if (!isValidSuiObjectId(normalized)) return null;

    const { details, status } = await rpc.getObject(normalized);
    if (is(details, SuiObject) && status === 'Exists') {
        return {
            label: 'object',
            results: [
                {
                    id: details.reference.objectId,
                    label: details.reference.objectId,
                    type: 'object',
                },
            ],
        };
    }
    return null;
};

const getResultsForAddress = async (rpc: JsonRpcProvider, query: string) => {
    const normalized = normalizeSuiObjectId(query);
    if (!isValidSuiAddress(normalized) || isGenesisLibAddress(normalized))
        return null;

    const [from, to] = await Promise.all([
        rpc.getTransactions({ FromAddress: normalized }, null, 1),
        rpc.getTransactions({ ToAddress: normalized }, null, 1),
    ]);
    if (from.data?.length || to.data?.length) {
        return {
            label: 'address',
            results: [
                {
                    id: normalized,
                    label: normalized,
                    type: 'address',
                },
            ],
        };
    }
    return null;
};

export function useSearch(query: string) {
    const rpc = useRpc();

    return useQuery(
        ['search', query],
        async () => {
            const results = await Promise.all([
                getResultsForTransaction(rpc, query),
                getResultsForAddress(rpc, query),
                getResultsForObject(rpc, query),
            ]);

            return results.filter(Boolean) as Result[];
        },
        {
            enabled: !!query,
            cacheTime: 10000,
        }
    );
}
