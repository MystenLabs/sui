// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import {
    isValidTransactionDigest,
    isValidSuiAddress,
    isValidSuiObjectId,
    normalizeSuiObjectId,
    is,
    SuiObject,
} from '@mysten/sui.js';
import { useQuery } from '@tanstack/react-query';

import { useRpc } from '~/hooks/useRpc';
import { isGenesisLibAddress } from '~/utils/api/searchUtil';

export function useSearch(query: string) {
    const rpc = useRpc();

    const getResultsForTransaction = async () => {
        if (isValidTransactionDigest(query)) {
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
        }
        return null;
    };

    const getResultsForObject = async () => {
        const normalized = normalizeSuiObjectId(query);
        if (isValidSuiObjectId(normalized)) {
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
        }
        return null;
    };

    const getResultsForAddress = async () => {
        if (isValidSuiAddress(query) && !isGenesisLibAddress(query)) {
            const [from, to] = await Promise.all([
                rpc.getTransactions({ FromAddress: query }, null, 1),
                rpc.getTransactions({ ToAddress: query }, null, 1),
            ]);
            if (from.data?.length || to.data?.length) {
                return {
                    label: 'address',
                    results: [
                        {
                            id: query,
                            label: query,
                            type: 'address',
                        },
                    ],
                };
            }
        }
        return null;
    };

    return useQuery(
        ['search', query],
        async () => {
            const results = await Promise.all([
                getResultsForTransaction(),
                getResultsForAddress(),
                getResultsForObject(),
            ]);
            return results.filter(Boolean);
        },
        {
            enabled: !!query,
            cacheTime: 10e3,
        }
    );
}
