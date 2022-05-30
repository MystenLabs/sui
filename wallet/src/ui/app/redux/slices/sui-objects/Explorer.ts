// Copyright (c) 2022, Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import { API_ENV, DEFAULT_API_ENV } from '_app/ApiProvider';

import type { ObjectId } from '@mysten/sui.js';

const API_ENV_TO_EXPLORER_URL: Record<API_ENV, string | undefined> = {
    [API_ENV.local]: process.env.EXPLORER_URL_LOCAL,
    [API_ENV.devNet]: process.env.EXPLORER_URL_DEV_NET,
};

function getDefaultUrl() {
    const url = API_ENV_TO_EXPLORER_URL[DEFAULT_API_ENV];
    if (!url) {
        throw new Error(`Url for API_ENV ${DEFAULT_API_ENV} is not defined`);
    }
    return url;
}

const DEFAULT_EXPLORER_URL = getDefaultUrl();

export class Explorer {
    private static _url = DEFAULT_EXPLORER_URL;

    public static getObjectUrl(objectID: ObjectId) {
        return new URL(`/objects/${objectID}`, Explorer._url).href;
    }
}
