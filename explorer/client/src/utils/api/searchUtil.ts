// Copyright (c) 2022, Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import {
    isValidTransactionDigest,
    isValidSuiAddress,
    type SuiTransactionResponse,
    type GetObjectDataResponse,
    type SuiObjectInfo,
} from '@mysten/sui.js';

import { DefaultRpcClient as rpc, type Network } from './DefaultRpcClient';

const getDataOnTx = async (input: string, network: Network | string) => {
    if (!isValidTransactionDigest(input)) return null;

    return await rpc(network)
        .getTransactionWithEffects(input)
        .then((data) => ({
            input: input,
            category: 'transactions',
            result: data,
        }))
        .catch((err) => {
            console.error(err);
            return null;
        });
};

const getDataOnAddress = async (input: string, network: Network | string) => {
    if (!isValidSuiAddress(input) && !isGenesisLibAddress(input)) return null;

    return await rpc(network)
        .getObjectsOwnedByAddress(input)
        .then((data) => {
            if (data.length <= 0) throw new Error('No objects for Address');

            return {
                input: input,
                category: 'addresses',
                result: data,
            };
        })
        .catch((err) => {
            console.error(err);
            return null;
        });
};

const getDataOnObject = async (input: string, network: Network | string) => {
    if (!isValidSuiAddress(input) && !isGenesisLibAddress(input)) return null;

    return await rpc(network)
        .getObject(input)
        .then((data) => {
            if (data.status !== 'Exists') {
                throw new Error('no object found');
            }
            return {
                input: input,
                category: 'objects',
                result: data,
            };
        })
        .catch((err) => {
            console.error(err);
            return null;
        });
};

export const navigateWithCategory = async (
    input: string,
    category: string,
    network: string,
    objectId?: string
): Promise<{
    input: string;
    category: string;
    result: SuiTransactionResponse | GetObjectDataResponse | SuiObjectInfo[];
} | null> => {
    switch (category) {
        case 'transaction':
            return getDataOnTx(input, network);
        case 'object':
            return getDataOnObject(input, network);
        case 'address':
            return getDataOnAddress(input, network);
        case 'owner':
            // The owner could be an Object or an Address
            // and a given Object can share its ID with another Address

            if (!objectId) {
                // If no Object ID provided, raise an error and then try for object and then address result
                console.error('Object ID was not provided');
                const objResult = await getDataOnObject(input, network);
                return objResult ? objResult : getDataOnAddress(input, network);
            }

            // Otherwise...
            // We take the ID (the value of input) and see if there is a matching Address
            const addResult = await getDataOnAddress(input, network);

            // If there is a matching Address, this could still be a coincidence
            // and the true owner may be an Object
            // So, we check that the Address has the Object ID in its Owned Objects
            if (
                addResult &&
                addResult.result.filter((el) => el.objectId === objectId)
                    .length > 0
            )
                return addResult;

            // If the owner is not an Address, then it is an Object
            return await getDataOnObject(input, network);

        default:
            return null;
    }
};

export const isGenesisLibAddress = (value: string): boolean =>
    /^(0x|0X)0{0,39}[12]$/.test(value);
