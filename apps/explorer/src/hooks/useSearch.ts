// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import {
    isValidTransactionDigest,
    type SuiTransactionResponse,
    type JsonRpcProvider,
} from '@mysten/sui.js';
import { useQuery } from '@tanstack/react-query';

import { useRpc } from '~/hooks/useRpc';

type SearchResult = {
    category: 'address' | 'object' | 'transaction';
    results: any;
};

// const getAddressData = async (address: string, rpc: JsonRpcProvider) => {
//     console.log('getting address data', address);

//     const result: SearchResult = { category: 'address', results: null };
//     console.log('is valid address', isValidSuiAddress(address));
//     if (!isValidSuiAddress(address) || isGenesisLibAddress(address))
//         return result;

//     result.data = await rpc.getObjectsOwnedByAddress(address);
//     return result;
// };

// const getObjectData = async (object: string, rpc: JsonRpcProvider) => {
//     const data = await rpc.getObject(object);
//     const result: SearchResult = { category: 'object', results: null };
//     if (data.status === 'NotExists') return result;
//     result.data = data;
//     return result;
// };

const getTxData = async (tx: string, rpc: JsonRpcProvider) => {
    const version = await rpc.getRpcApiVersion();
    const result: SearchResult = { category: 'transaction', results: [] };
    console.log(
        'is valid transaction digest',
        isValidTransactionDigest(tx, version)
    );
    if (!isValidTransactionDigest(tx, version)) return result;
    const txdata: SuiTransactionResponse = await rpc.getTransactionWithEffects(
        tx
    );

    result.results.push({
        id: txdata.effects.transactionDigest,
        label: txdata.effects.transactionDigest,
    });
    console.log(result);
    return result;
};

const handleSearch = async (rpc: JsonRpcProvider, query: string) => {
    if (!query) return [];
    let results = [];

    const txData = await getTxData(query, rpc);

    results.push(txData);

    return results;

    // const txData: any = await getTxData(query, rpc);
    // const addressData: any = await getAddressData(query, rpc);
    // const objectData: any = await getObjectData(query, rpc);
    // // const results = await Promise.allSettled([txData, addressData, objectData]);
    // console.log(txData, addressData, objectData);
    // const r = results.map((result: any) => result.value);
    // const [txs, addresses, objects] = r;
    // return { transaction: txs, address: addresses, object: objects };
};

export function useSearch(query: string) {
    const rpc = useRpc();

    let results = [];

    return useQuery(['search', query], async () => {
        const results = await handleSearch(rpc, query);
        console.log(results);
        return results;
    });
}
