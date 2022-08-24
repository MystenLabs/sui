// Copyright (c) 2022, Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import latestTxData from './latest_transactions.json';
import mockData from './mock_data.json';
import mockOwnedObjectData from './owned_object.json';
import mockTxData from './tx_for_id.json';

const navigateWithCategory = async (
    input: string,
    category: string,
    network: string
): Promise<{
    input: string;
    category: string;
    result: object;
} | null> => {
    // If Object or Transaction, use findDataFromID
    if (['object', 'transaction'].includes(category)) {
        const data = await findDataFromID(input, false);

        if (data?.category === category) {
            return {
                input: input,
                category:
                    data?.category === 'object' ? 'objects' : 'transactions',
                result: data,
            };
        }
        return null;
    }

    // If Address, use findOwnedObjectsfromID:
    if (category === 'address') {
        const data = await findDataFromID(input, false);
        const ownedObjects = await findOwnedObjectsfromID(input);

        if (ownedObjects && ownedObjects.length > 0) {
            return {
                input: input,
                category: 'addresses',
                result: data,
            };
        }
    }

    // If Owner, could be Object or Address:
    if (category === 'owner') {
        const data = await findDataFromID(input, false);

        // First check is Object:
        if (data?.category === 'object') {
            return {
                input: input,
                category: 'objects',
                result: data,
            };
        }

        //Then check is Address:
        const ownedObjects = await findOwnedObjectsfromID(input);
        if (ownedObjects && ownedObjects.length > 0) {
            return {
                input: input,
                category: 'addresses',
                result: data,
            };
        }
    }

    return null;
};

const findDataFromID = (targetID: string | undefined, state: any) =>
    state?.category !== undefined
        ? state
        : mockData.data.find(({ id }) => id === targetID);

const findOwnedObjectsfromID = (targetID: string | undefined) =>
    mockOwnedObjectData?.data?.find(({ id }) => id === targetID)?.objects;

const getAllMockTransaction = () => latestTxData.data;

const findTxfromID = (targetID: string | undefined) =>
    mockTxData!.data!.find(({ id }) => id === targetID);

const findTxDatafromID = (targetID: string | undefined) =>
    latestTxData!.data!.find(({ txId }) => txId === targetID);

export {
    findDataFromID,
    navigateWithCategory,
    findOwnedObjectsfromID,
    findTxfromID,
    findTxDatafromID,
    getAllMockTransaction,
};
