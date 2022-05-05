// Copyright (c) 2022, Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import mockObjectData from './mock_data.json';
import mockOwnedObjectData from './owned_object.json';

const navigateWithUnknown = async (input: string, navigate: Function) => {
    const data = findDataFromID(input, false);
    const ownedObjects = findOwnedObjectsfromID(input);

    if (data?.category === 'transaction') {
        navigate(`../transactions/${input}`, { state: data });
    } else if (data?.category === 'object') {
        navigate(`../objects/${input}`, { state: data });
    } else if (ownedObjects && ownedObjects.length > 0) {
        navigate(`../addresses/${input}`, { state: data });
    } else {
        navigate(`../missing/${input}`);
    }
};

const findDataFromID = (targetID: string | undefined, state: any) =>
    state?.category !== undefined
        ? state
        : mockObjectData.data.find(({ id }) => id === targetID);

const findOwnedObjectsfromID = (targetID: string | undefined) =>
    mockOwnedObjectData?.data?.find(({ id }) => id === targetID)?.objects;

export { findDataFromID, navigateWithUnknown, findOwnedObjectsfromID };
