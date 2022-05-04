// Copyright (c) 2022, Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import type { ObjectExistsInfo } from 'sui.js';

export function parseImageURL(data: any): string {
    if (data?.contents?.url?.fields) {
        return data.contents.url.fields['url'];
    }
    // TODO: Remove Legacy format
    if (data?.contents?.display) {
        return data.contents.display;
    }
    return '';
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
