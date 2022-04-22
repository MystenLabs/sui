// Copyright (c) 2022, Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import { DefaultRpcClient as rpc } from './DefaultRpcClient';

export const navigateWithUnknown = async (
    input: string,
    navigate: Function
) => {
    // TODO - replace multi-request search with backend function when ready
    const addrPromise = rpc.getOwnedObjectRefs(input).then((data) => {
        if (data.length <= 0) throw new Error('No objects for Address');

        return {
            category: 'addresses',
            data: data,
        };
    });

    const objInfoPromise = rpc.getObjectInfo(input).then((data) => ({
        category: 'objects',
        data: data,
    }));

    const txDetailsPromise = rpc.getTransaction(input).then((data) => ({
        category: 'transactions',
        data: data,
    }));

    return (
        Promise.any([objInfoPromise, addrPromise, txDetailsPromise])
            .then((pac) => {
                if (
                    pac?.data &&
                    (pac?.category === 'objects' ||
                        pac?.category === 'addresses' ||
                        pac?.category === 'transactions')
                ) {
                    navigate(`../${pac.category}/${input}`, {
                        state: pac.data,
                    });
                } else {
                    throw new Error(
                        'Something wrong with navigateWithUnknown function'
                    );
                }
            })
            //if none of the queries find a result, show missing page
            .catch((error) => {
                console.log(error);
                navigate(`../missing/${input}`);
            })
    );
};
