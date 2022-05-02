// Copyright (c) 2022, Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

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
