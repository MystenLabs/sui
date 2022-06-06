// Copyright (c) 2022, Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import { JsonRpcProvider, RawSigner } from '@mysten/sui.js';

import type { Ed25519Keypair } from '@mysten/sui.js';

export enum API_ENV {
    local = 'local',
    devNet = 'devNet',
    staging = 'staging',
}

type EnvInfo = {
    name: string;
    color: string;
};
export const API_ENV_TO_INFO: Record<API_ENV, EnvInfo> = {
    [API_ENV.local]: { name: 'Local', color: '#000' },
    [API_ENV.devNet]: { name: 'DevNet', color: '#666' },
    [API_ENV.staging]: { name: 'Staging', color: '#999' },
};

export const ENV_TO_API: Record<API_ENV, string | undefined> = {
    [API_ENV.local]: process.env.API_ENDPOINT_LOCAL,
    [API_ENV.devNet]: process.env.API_ENDPOINT_DEV_NET,
    [API_ENV.staging]: process.env.API_ENDPOINT_STAGING,
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
    private _signer: RawSigner | null = null;

    constructor() {
        this._apiProvider = new JsonRpcProvider(DEFAULT_API_ENDPOINT);
    }

    public get instance() {
        return this._apiProvider;
    }

    public getSignerInstance(keypair: Ed25519Keypair): RawSigner {
        if (!this._signer) {
            this._signer = new RawSigner(keypair, this._apiProvider);
        }
        return this._signer;
    }
}
