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
    network: string
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
            // The owner could be an object or an address
            // first check for an object...
            const objResult = await getDataOnObject(input, network);
            // if no object check for an address
            return objResult ? objResult : getDataOnAddress(input, network);
        default:
            return null;
    }
};

export const isGenesisLibAddress = (value: string): boolean =>
    /^(0x|0X)0{0,39}[12]$/.test(value);
