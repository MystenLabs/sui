// Copyright (c) 2022, Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import { isValidTransactionDigest, isValidSuiAddress } from '@mysten/sui.js';

import { DefaultRpcClient as rpc, type Network } from './DefaultRpcClient';

export const navigateWithUnknown = async (
    input: string,
    navigate: Function,
    network: Network | string
) => {
    let searchPromises = [];

    if (isValidTransactionDigest(input)) {
        searchPromises.push(
            rpc(network)
                .getTransactionWithEffects(input)
                .then((data) => ({
                    category: 'transactions',
                    data: data,
                }))
        );
    }

    // object IDs and addresses can't be distinguished just by the string, so search both.
    // allow navigating to the standard Move packages at 0x1 & 0x2 as a convenience
    // Get Search results for a given query from both the object and address index
    else if (isValidSuiAddress(input) || isGenesisLibAddress(input)) {
        const addrObjPromise = Promise.allSettled([
            rpc(network)
                .getObjectsOwnedByAddress(input)
                .then((data) => {
                    if (data.length <= 0)
                        throw new Error('No objects for Address');

                    return {
                        category: 'addresses',
                        data: data,
                    };
                }),
            rpc(network)
                .getObject(input)
                .then((data) => {
                    if (data.status !== 'Exists') {
                        throw new Error('no object found');
                    }
                    return {
                        category: 'objects',
                        data: data,
                    };
                }),
        ]).then((results) => {
            // return only the successful results
            const searchResult = results
                .filter((result: any) => result.status === 'fulfilled')
                .map((data: any) => data.value);
            // return array of objects if results are found for both address and object, return just the data obj if only one is found
            return searchResult.length > 1 ? searchResult : searchResult[0];
        });
        searchPromises.push(addrObjPromise);
    }

    if (searchPromises.length === 0) {
        navigate(`../error/all/${encodeURIComponent(input)}`);
        return;
    }

    return (
        Promise.any(searchPromises)
            .then((pac: any) => {
                // Redirect to search result page if there are multiple categories with the same query
                if (Array.isArray(pac)) {
                    navigate(`../search-result/${encodeURIComponent(input)}`);
                    return;
                }

                if (
                    pac?.data &&
                    (pac?.category === 'objects' ||
                        pac?.category === 'addresses' ||
                        pac?.category === 'transactions')
                ) {
                    navigate(
                        `../${pac.category}/${encodeURIComponent(input)}`,
                        {
                            state: pac.data,
                        }
                    );
                } else {
                    throw new Error(
                        'Something wrong with navigateWithUnknown function'
                    );
                }
            })
            //if none of the queries find a result, show missing page
            .catch((error) => {
                // encode url in
                navigate(`../error/missing/${encodeURIComponent(input)}`);
            })
    );
};

export const isGenesisLibAddress = (value: string): boolean =>
    /^(0x|0X)0{0,39}[12]$/.test(value);
