// Copyright (c) 2022, Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import { getObjectType, getMoveObjectType } from '@mysten/sui.js';

import type { GetObjectDataResponse } from '@mysten/sui.js';

export function parseImageURL(data: any): string {
    return (
        data?.url ||
        // TODO: Remove Legacy format
        data?.display ||
        data?.contents?.display ||
        ''
    );
}

export function parseObjectType(data: GetObjectDataResponse): string {
    // TODO: define better naming and typing here
    const dataType = getObjectType(data);
    if (dataType === 'package') {
        return 'Move Package';
    }
    if (dataType === 'moveObject') {
        return getMoveObjectType(data)!;
    }
    return 'unknown';
}
