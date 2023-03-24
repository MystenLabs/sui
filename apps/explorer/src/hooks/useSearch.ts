// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import { useFeature } from '@growthbook/growthbook-react';
import { useRpcClient } from '@mysten/core';
import {
    isValidTransactionDigest,
    isValidSuiAddress,
    isValidSuiObjectId,
    normalizeSuiObjectId,
    is,
    SuiObjectData,
    type JsonRpcProvider,
    getTransactionDigest,
} from '@mysten/sui.js';
import { useQuery } from '@tanstack/react-query';

import { GROWTHBOOK_FEATURES } from '~/utils/growthbook';

const isGenesisLibAddress = (value: string): boolean =>
    /^(0x|0X)0{0,39}[12]$/.test(value);

type Result = {
    label: string;
    results: { id: string; label: string; type: string }[];
};

const getResultsForTransaction = async (
    rpc: JsonRpcProvider,
    query: string
) => {
    if (!isValidTransactionDigest(query)) return null;

    const txdata = await rpc.getTransaction({ digest: query });
    return {
        label: 'transaction',
        results: [
            {
                id: getTransactionDigest(txdata),
                label: getTransactionDigest(txdata),
                type: 'transaction',
            },
        ],
    };
};

const getResultsForObject = async (rpc: JsonRpcProvider, query: string) => {
    const normalized = normalizeSuiObjectId(query);
    if (!isValidSuiObjectId(normalized)) return null;

    const { data, error } = await rpc.getObject({ id: normalized });
    if (is(data, SuiObjectData) && !error) {
        return {
            label: 'object',
            results: [
                {
                    id: data.objectId,
                    label: data.objectId,
                    type: 'object',
                },
            ],
        };
    }

    return null;
};

const getResultsForCheckpoint = async (rpc: JsonRpcProvider, query: string) => {
    const { digest } = await rpc.getCheckpoint({ id: query });
    if (digest) {
        return {
            label: 'checkpoint',
            results: [
                {
                    id: digest,
                    label: digest,
                    type: 'checkpoint',
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
        rpc.queryTransactions({
            filter: { FromAddress: normalized },
            limit: 1,
        }),
        rpc.queryTransactions({ filter: { ToAddress: normalized }, limit: 1 }),
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
    const rpc = useRpcClient();
    const checkpointsEnabled = useFeature(
        GROWTHBOOK_FEATURES.EPOCHS_CHECKPOINTS
    ).on;

    return useQuery(
        ['search', query],
        async () => {
            const results = (
                await Promise.allSettled([
                    getResultsForTransaction(rpc, query),
                    ...(checkpointsEnabled
                        ? [getResultsForCheckpoint(rpc, query)]
                        : []),
                    getResultsForAddress(rpc, query),
                    getResultsForObject(rpc, query),
                ])
            ).filter(
                (r) => r.status === 'fulfilled' && r.value
            ) as PromiseFulfilledResult<Result>[];

            return results.map(({ value }) => value);
        },
        {
            enabled: !!query,
            cacheTime: 10000,
        }
    );
}
