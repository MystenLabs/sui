// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import { SuiObjectResponse, SuiMoveObject, SuiObjectData } from '@mysten/sui.js';
import { normalizeSuiAddress } from '@mysten/sui.js/utils';

export const camelCase = (string: string) => string.replace(/(_\w)/g, (g) => g[1].toUpperCase());

export const parseObjectDataResponse = (response: SuiObjectResponse | undefined) =>
    ((response?.data as SuiObjectData)?.content as SuiMoveObject)?.fields;

export const parseRegistryResponse = (response: SuiObjectResponse | undefined): any => {
    const fields = parseObjectDataResponse(response)?.value?.fields || {};

    const object = Object.fromEntries(
        Object.entries({ ...fields }).map(([key, val]) => [camelCase(key), val]),
    );

    if (response?.data?.objectId) {
        object.id = response.data.objectId;
    }

    delete object.data;

    const data = (fields.data?.fields.contents || []).reduce(
        (acc: Record<string, any>, c: Record<string, any>) => {
            const key = c.fields.key;
            const value = c.fields.value;

            return {
                ...acc,
                [camelCase(key)]:
                    c.type.includes('Address') || key === 'addr'
                        ? normalizeSuiAddress(value)
                        : value,
            };
        },
        {},
    );

    return { ...object, ...data };
};
