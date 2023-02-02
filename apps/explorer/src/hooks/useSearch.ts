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

    return useQuery(
        ['search', query],
        () => {
            const promises = [];

            if (isValidTransactionDigest(query)) {
                promises.push(async () => {
                    const txdata = await rpc.getTransactionWithEffects(query);
                    return [
                        {
                            label: 'transaction',
                            results: [
                                {
                                    id: txdata.certificate.transactionDigest,
                                    label: txdata.certificate.transactionDigest,
                                    type: 'transaction',
                                },
                            ],
                        },
                    ];
                });
            }

            if (isValidSuiAddress(query) && !isGenesisLibAddress(query)) {
                promises.push(async () => {
                    const [from, to] = await Promise.all([
                        rpc.getTransactions({ FromAddress: query }, null, 1),
                        rpc.getTransactions({ ToAddress: query }, null, 1),
                    ]);
                    if (from.data.length || to.data.length) {
                        return [
                            {
                                label: 'address',
                                results: [
                                    {
                                        id: query,
                                        label: query,
                                        type: 'address',
                                    },
                                ],
                            },
                        ];
                    } else {
                        throw new Error('not a valid address');
                    }
                });
            }

            const normalized = normalizeSuiObjectId(query);
            if (isValidSuiObjectId(normalized)) {
                promises.push(async () => {
                    const { details, status } = await rpc.getObject(normalized);

                    if (is(details, SuiObject) && status === 'Exists') {
                        return [
                            {
                                label: 'object',
                                results: [
                                    {
                                        id: details.reference.objectId,
                                        label: details.reference.objectId,
                                        type: 'object',
                                    },
                                ],
                            },
                        ];
                    }
                });
            }
            return Promise.all(promises.map((p) => p())).catch(() => []);
        },
        {
            enabled: !!query,
            cacheTime: 10e3,
        }
    );
}
