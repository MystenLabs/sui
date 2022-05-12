// Copyright (c) 2022, Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import type { ObjectExistsInfo } from '@mysten/sui.js';

export function parseImageURL(data: any): string {
    return (
        //Render Image for Preview Cards
        data?.contents?.fields?.url?.fields?.url ||
        //Render Image for Object Results
        data?.contents?.url?.fields?.url ||
        // TODO: Remove Legacy format
        data?.contents?.display ||
        ''
    );
}

export function parseObjectType(data: ObjectExistsInfo): string {
    // TODO: define better naming and typing here
    if (data.objectType === 'movePackage') {
        return 'Move Package';
    }
    if (data.objectType === 'moveObject') {
        return data.object.contents.type;
    }
    return 'unknown';
}
