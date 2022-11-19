// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import {
    getObjectFields,
    isSuiObject,
    type GetObjectDataResponse,
} from '@mysten/sui.js';
import { useQuery } from '@tanstack/react-query';
import { useMemo } from 'react';

import { api } from '../redux/store/thunk-extras';

type NFTMetaData = {
    name: string | false;
    description: string | false;
    url?: string;
    objectID?: string;
} | null;

const transformNFTMetaData = (
    objectDetails: GetObjectDataResponse
): NFTMetaData => {
    const { details } = objectDetails || {};
    if (!isSuiObject(details)) return null;
    const fields = getObjectFields(objectDetails);

    if (!fields?.url) return null;
    return {
        description:
            typeof fields.description === 'string' && fields.description,
        name: typeof fields.name === 'string' && fields.name,
        url: fields.url,
        objectID: fields.id.id,
    };
};

export function useGetNFTMetaData(objectID?: string | null): NFTMetaData {
    const data = useQuery(['nFTMetaData', objectID], async () => {
        if (!objectID) {
            return null;
        }
        return api.instance.fullNode.getObject(objectID);
    });

    const nfTMeta = useMemo(
        () => (data.data ? transformNFTMetaData(data.data) : null),
        [data]
    );

    return nfTMeta;
}
