// Copyright (c) 2022, Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import { JsonRpcProvider } from '@mysten/sui.js';

export enum API_ENV {
    local = 'local',
    devNet = 'devNet',
}

export const ENV_TO_API: Record<API_ENV, string | undefined> = {
    [API_ENV.local]: process.env.API_ENDPOINT_LOCAL,
    [API_ENV.devNet]: process.env.API_ENDPOINT_DEV_NET,
};

function getDefaultApiEnv() {
    const apiEnv = process.env.API_ENV;
    if (apiEnv && !Object.keys(API_ENV).includes(apiEnv)) {
        throw new Error(`Unknown environment variable API_ENV, ${apiEnv}`);
    }
    return apiEnv ? API_ENV[apiEnv as keyof typeof API_ENV] : API_ENV.devNet;
}

function getDefaultAPI(env: API_ENV) {
    const apiEndpoint = ENV_TO_API[env];
    if (!apiEndpoint) {
        throw new Error(`API endpoint not found for API_ENV ${env}`);
    }
    return apiEndpoint;
}

export const DEFAULT_API_ENV = getDefaultApiEnv();
export const DEFAULT_API_ENDPOINT = getDefaultAPI(DEFAULT_API_ENV);

export default class ApiProvider {
    private _apiProvider: JsonRpcProvider;

    constructor() {
        this._apiProvider = new JsonRpcProvider(DEFAULT_API_ENDPOINT);
    }

    public get instance() {
        return this._apiProvider;
    }
}
