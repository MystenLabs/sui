// Copyright (c) 2022, Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import { RawSigner, JsonRpcProvider } from '@mysten/sui.js';

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

type ApiEndpoints = {
    gateway: string;
    fullNode: string;
};
export const API_ENV_TO_INFO: Record<API_ENV, EnvInfo> = {
    [API_ENV.local]: { name: 'Local', color: '#9064ff' },
    [API_ENV.devNet]: { name: 'DevNet', color: '#29b6af' },
    [API_ENV.staging]: { name: 'Staging', color: '#ff4a8d' },
};

export const ENV_TO_API: Record<API_ENV, ApiEndpoints> = {
    [API_ENV.local]: {
        gateway: process.env.API_ENDPOINT_LOCAL || '',
        fullNode: process.env.API_ENDPOINT_LOCAL_FULLNODE || '',
    },
    [API_ENV.devNet]: {
        gateway: process.env.API_ENDPOINT_DEV_NET || '',
        fullNode: process.env.API_ENDPOINT_DEV_NET_FULLNODE || '',
    },
    [API_ENV.staging]: {
        gateway: process.env.API_ENDPOINT_STAGING || '',
        fullNode: process.env.API_ENDPOINT_STAGING_FULLNODE || '',
    },
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
    if (
        !apiEndpoint ||
        apiEndpoint.gateway === '' ||
        apiEndpoint.fullNode === ''
    ) {
        throw new Error(`API endpoint not found for API_ENV ${env}`);
    }
    return apiEndpoint;
}

export const DEFAULT_API_ENV = getDefaultApiEnv();
export const DEFAULT_API_ENDPOINT = getDefaultAPI(DEFAULT_API_ENV);

export default class ApiProvider {
    private _apiProvider: JsonRpcProvider;
    private _apiFullNodeProvider: JsonRpcProvider;
    private _signer: RawSigner | null = null;

    constructor() {
        this._apiProvider = new JsonRpcProvider(DEFAULT_API_ENDPOINT.gateway);
        this._apiFullNodeProvider = new JsonRpcProvider(
            DEFAULT_API_ENDPOINT.fullNode
        );
    }

    public setNewJsonRpcProvider(apiEnv: API_ENV) {
        this._apiProvider = new JsonRpcProvider(getDefaultAPI(apiEnv).gateway);
        this._apiFullNodeProvider = new JsonRpcProvider(
            getDefaultAPI(apiEnv).fullNode
        );
        this._signer = null;
    }

    public get instance() {
        return {
            gateway: this._apiProvider,
            fullNode: this._apiFullNodeProvider,
        };
    }

    public getSignerInstance(keypair: Ed25519Keypair): RawSigner {
        if (!this._signer) {
            this._signer = new RawSigner(keypair, this._apiProvider);
        }
        return this._signer;
    }
}
