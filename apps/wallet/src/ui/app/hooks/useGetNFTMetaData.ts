// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import { getObjectFields,  isSuiObject } from '@mysten/sui.js';
import { useQuery, type UseQueryResult } from '@tanstack/react-query';

import { api } from '../redux/store/thunk-extras';

type NFTMetaData = [
    name: string,
    description: string,
    url: string,
    queryResult: UseQueryResult
];

export function useGetNFTMetaData (objectIDs: string[]) { 
    const { data, isError } = useQuery(
        ['getNFTMetaData', objectIDs], 
        async () => {
            if (!objectIDs || objectIDs.length === 0) {
                return [];
            }
            const response = await api.instance.fullNode.getObjectBatch(
                objectIDs
            );
            const txObjects = response.filter(
                ({ status }) => status === 'Exists'
            );
            return txObjects;
        });

        if(isError) {
            return [];
        }

        return data?.map((objectDetails) => {
           if(!isSuiObject(objectDetails)) return null;
           const fields =  getObjectFields(objectDetails);
            return {
                ...(fields &&
                    fields.url && {
                        description:
                            typeof fields.description === 'string' &&
                            fields.description,
                        name: typeof fields.name === 'string' && fields.name,
                        url: fields.url,
                    }),
            };
        });


        
}