// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import {
    isValidTransactionDigest,
    type SuiTransactionResponse,
    type JsonRpcProvider,
    isValidSuiAddress,
    isValidSuiObjectId,
    normalizeSuiObjectId,
    type GetObjectDataResponse,
    is,
    SuiObject,
} from '@mysten/sui.js';
import { useQuery } from '@tanstack/react-query';

import { useRpc } from '~/hooks/useRpc';
import { isGenesisLibAddress } from '~/utils/api/searchUtil';

const handleSearch = async (rpc: JsonRpcProvider, query: string) => {
    const version = await rpc.getRpcApiVersion();
    let results: any = {};
    if (
        isValidTransactionDigest(
            query,
            version?.major === 0 && version?.minor < 18 ? 'base64' : 'base58'
        )
    ) {
        const txdata: SuiTransactionResponse =
            await rpc.getTransactionWithEffects(query);
        results.transaction = [
            {
                id: txdata.certificate.transactionDigest,
                label: txdata.certificate.transactionDigest,
                type: 'transaction',
            },
        ];
    }

    if (isValidSuiAddress(query) && !isGenesisLibAddress(query)) {
        const data = await rpc.getObjectsOwnedByAddress(query);
        if (data.length) {
            results.address = [
                {
                    id: query,
                    label: query,
                    type: 'address',
                },
            ];
            results.object = data
                .map((obj) => ({
                    id: obj.objectId,
                    label: obj.objectId,
                    type: 'object',
                }))
                .slice(0, 5);
        }
    }

    if (isValidSuiObjectId(query)) {
        const { details, status } = (await rpc.getObject(
            normalizeSuiObjectId(query)
        )) as GetObjectDataResponse;

        if (is(details, SuiObject) && status === 'Exists') {
            let name;
            if (details.data.dataType === 'moveObject') {
                name = details.data.fields.name;
            }
            results.object = [
                {
                    id: details.reference.objectId,
                    label: `${name ? `${name} ` : ''}${
                        details.reference.objectId
                    }`,
                    type: 'object',
                },
            ];
        }
    }

    return results;
};

export function useSearch(query: string) {
    const rpc = useRpc();

    return useQuery(
        ['search', query],
        () => {
            return handleSearch(rpc, query);
        },
        {
            enabled: !!query,
        }
    );
}
