// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import {
    isValidTransactionDigest,
    type SuiTransactionResponse,
    type JsonRpcProvider,
    isValidSuiAddress,
    isValidSuiObjectId,
    normalizeSuiObjectId,
} from '@mysten/sui.js';
import { useQuery } from '@tanstack/react-query';

import { useRpc } from '~/hooks/useRpc';
import { isGenesisLibAddress } from '~/utils/api/searchUtil';

const handleSearch = async (rpc: JsonRpcProvider, query: string) => {
    const version = await rpc.getRpcApiVersion();
    let results: any = {};

    if (isValidTransactionDigest(query, version)) {
        const txdata: SuiTransactionResponse =
            await rpc.getTransactionWithEffects(query);
        results.transaction = [
            {
                id: txdata.certificate.transactionDigest,
                label: txdata.certificate.transactionDigest,
                category: 'transaction',
            },
        ];
    }

    if (isValidSuiAddress(query) && !isGenesisLibAddress(query)) {
        const data = await rpc.getObjectsOwnedByAddress(query);
        results = {
            ...results,
            address: [
                {
                    id: query,
                    label: query,
                    category: 'address',
                },
            ],
            object: data
                .map((obj) => ({
                    id: obj.objectId,
                    label: obj.objectId,
                    category: 'object',
                }))
                .slice(0, 5),
        };
    }

    if (isValidSuiObjectId(query)) {
        const { status, details } = await rpc.getObject(
            normalizeSuiObjectId(query)
        );

        if (status === 'Exists') {
            results.object = [
                ...(results.object || []),
                [
                    {
                        id: details.reference.objectId,
                        label: details.reference.objectId,
                        category: 'object',
                    },
                ],
            ];
        }
    }

    return results;
};

export function useSearch(query: string) {
    const rpc = useRpc();
    return useQuery(['search', query], () => handleSearch(rpc, query), {
        enabled: !!query,
    });
}
