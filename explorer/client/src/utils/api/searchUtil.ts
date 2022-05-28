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
    else if (isValidSuiAddress(input) || isGenesisLibAddress(input)) {
        const addrPromise = rpc(network)
            .getObjectsOwnedByAddress(input)
            .then((data) => {
                if (data.length <= 0) throw new Error('No objects for Address');

                return {
                    category: 'addresses',
                    data: data,
                };
            });
        const objInfoPromise = rpc(network)
            .getObject(input)
            .then((data) => {
                if (data.status !== 'Exists') {
                    throw new Error('no object found');
                }

                return {
                    category: 'objects',
                    data: data,
                };
            });

        searchPromises.push(addrPromise, objInfoPromise);
    }

    if (searchPromises.length === 0) {
        navigate(`../error/all/${encodeURIComponent(input)}`);
        return;
    }

    return (
        Promise.any(searchPromises)
            .then((pac) => {
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
