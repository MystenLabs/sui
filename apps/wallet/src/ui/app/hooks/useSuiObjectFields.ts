// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import { isSuiMoveObject } from '@mysten/sui.js';
import { useMemo } from 'react';

import type { SuiData } from '@mysten/sui.js';

const fieldsOrder: Record<string, number> = {
    name: 0,
    description: 1,
};
const forceOther = ['url'];
const allowedDetailedTypes = ['string', 'number', 'bigint', 'boolean'];

function sortKeys(a: string, b: string) {
    const aInForcedOrder = a in fieldsOrder;
    const bInForcedOrder = b in fieldsOrder;
    if (aInForcedOrder && !bInForcedOrder) {
        return -1;
    }
    if (bInForcedOrder && !aInForcedOrder) {
        return 1;
    }
    if (aInForcedOrder && bInForcedOrder) {
        return fieldsOrder[a] - fieldsOrder[b];
    }
    return a.localeCompare(b);
}

export default function useSuiObjectFields(data: SuiData) {
    const { fields = null } = isSuiMoveObject(data) ? data : {};
    return useMemo(() => {
        const keys: string[] = [];
        const otherKeys: string[] = [];
        if (fields) {
            const allKeys = Object.keys(fields);
            for (const aKey of allKeys) {
                const keyType = typeof fields[aKey];
                if (
                    allowedDetailedTypes.includes(keyType) &&
                    !forceOther.includes(aKey)
                ) {
                    keys.push(aKey);
                } else {
                    otherKeys.push(aKey);
                }
            }
        }
        keys.sort(sortKeys);
        return {
            keys,
            otherKeys,
        };
    }, [fields]);
}
