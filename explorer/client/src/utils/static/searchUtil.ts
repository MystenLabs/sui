// Copyright (c) 2022, Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import { SEARCH_CATEGORIES } from '../searchUtil';
import latestTxData from './latest_transactions.json';
import mockData from './mock_data.json';
import mockOwnedObjectData from './owned_object.json';
import mockTxData from './tx_for_id.json';

const navigateWithUnknown = async (
    input: string,
    navigate: Function,
    network: string
) => {
    const data = findDataFromID(input, false);
    const ownedObjects = findOwnedObjectsfromID(input);

    if (data?.category === 'transaction') {
        navigate(`../transactions/${input}`, { state: data });
    } else if (data?.category === 'object') {
        navigate(`../objects/${input}`, { state: data });
    } else if (ownedObjects && ownedObjects.length > 0) {
        navigate(`../addresses/${input}`, { state: data });
    } else {
        navigate(`../error/missing/${input}`);
    }
};

const navigateWithCategory = async (
    input: string,
    category: typeof SEARCH_CATEGORIES[number],
    network: string
): Promise<{
    input: string;
    category: typeof SEARCH_CATEGORIES[number];
    result: object;
} | null> => {
    if ([SEARCH_CATEGORIES[0], SEARCH_CATEGORIES[1]].includes(category)) {
        const data = await findDataFromID(input, false);

        if (data?.category === category) {
            return {
                input: input,
                category: category,
                result: data,
            };
        }
    } else {
        const data = await findDataFromID(input, false);
        const ownedObjects = await findOwnedObjectsfromID(input);

        if (ownedObjects && ownedObjects.length > 0) {
            return {
                input: input,
                category: category,
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
    navigateWithUnknown,
    navigateWithCategory,
    findOwnedObjectsfromID,
    findTxfromID,
    findTxDatafromID,
    getAllMockTransaction,
};
