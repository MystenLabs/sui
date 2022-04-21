// Copyright (c) 2022, Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import { DefaultRpcClient as rpc } from './SuiRpcClient';

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

    //if none of the queries find a result, show missing page
    return Promise.any([objInfoPromise, addrPromise])
        .then((pac) => {
            if (
                pac?.data &&
                (pac?.category === 'objects' || pac?.category === 'addresses')
            ) {
                navigate(`../${pac.category}/${input}`, { state: pac.data });
            } else {
                throw new Error(
                    'Something wrong with navigateWithUnknown function'
                );
            }
        })
        .catch((error) => {
            console.log(error);
            navigate(`../missing/${input}`);
        });
};
