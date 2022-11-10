// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import {
    RawSigner,
    JsonRpcProvider,
    LocalTxnDataSerializer,
} from '@mysten/sui.js';

import { growthbook } from './experimentation/feature-gating';
import { FEATURES } from './experimentation/features';
import { queryClient } from './helpers/queryClient';

import type { Keypair } from '@mysten/sui.js';

export enum API_ENV {
    local = 'local',
    devNet = 'devNet',
    staging = 'staging',
    testNet = 'testNet',
    customRPC = 'customRPC',
}

type EnvInfo = {
    name: string;
};

type ApiEndpoints = {
    fullNode: string;
    faucet: string;
} | null;
export const API_ENV_TO_INFO: Record<API_ENV, EnvInfo> = {
    [API_ENV.local]: { name: 'Local' },
    [API_ENV.devNet]: { name: 'Sui Devnet' },
    [API_ENV.staging]: { name: 'Sui Staging' },
    [API_ENV.customRPC]: { name: 'Custom RPC URL' },
    [API_ENV.testNet]: { name: 'Sui Testnet' },
};

export const ENV_TO_API: Record<API_ENV, ApiEndpoints> = {
    [API_ENV.local]: {
        fullNode: process.env.API_ENDPOINT_LOCAL_FULLNODE || '',
        faucet: process.env.API_ENDPOINT_LOCAL_FAUCET || '',
    },
    [API_ENV.devNet]: {
        fullNode: process.env.API_ENDPOINT_DEV_NET_FULLNODE || '',
        faucet: process.env.API_ENDPOINT_DEV_NET_FAUCET || '',
    },
    [API_ENV.staging]: {
        fullNode: process.env.API_ENDPOINT_STAGING_FULLNODE || '',
        faucet: process.env.API_ENDPOINT_STAGING_FAUCET || '',
    },
    [API_ENV.customRPC]: null,
    [API_ENV.testNet]: {
        fullNode: process.env.API_ENDPOINT_TEST_NET_FULLNODE || '',
        faucet: process.env.API_ENDPOINT_TEST_NET_FAUCET || '',
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
        apiEndpoint.fullNode === '' ||
        apiEndpoint.faucet === ''
    ) {
        throw new Error(`API endpoint not found for API_ENV ${env}`);
    }
    return apiEndpoint;
}

export const DEFAULT_API_ENV = getDefaultApiEnv();

type NetworkTypes = keyof typeof API_ENV;

export const generateActiveNetworkList = (): NetworkTypes[] => {
    const excludedNetworks: NetworkTypes[] = [];

    if (process.env.SHOW_STAGING !== 'false') {
        excludedNetworks.push(API_ENV.staging);
    }

    if (!growthbook.isOn(FEATURES.USE_TEST_NET_ENDPOINT)) {
        excludedNetworks.push(API_ENV.testNet);
    }

    if (!growthbook.isOn(FEATURES.USE_CUSTOM_RPC_URL)) {
        excludedNetworks.push(API_ENV.customRPC);
    }

    return Object.values(API_ENV).filter(
        (env) => !excludedNetworks.includes(env as keyof typeof API_ENV)
    );
};

export default class ApiProvider {
    private _apiFullNodeProvider?: JsonRpcProvider;
    private _signer: RawSigner | null = null;

    public setNewJsonRpcProvider(
        apiEnv: API_ENV = DEFAULT_API_ENV,
        customRPC?: string | null
    ) {
        // We also clear the query client whenever set set a new API provider:
        queryClient.clear();
        this._apiFullNodeProvider = new JsonRpcProvider(
            customRPC ?? getDefaultAPI(apiEnv).fullNode
        );
        this._signer = null;
    }

    public get instance() {
        if (!this._apiFullNodeProvider) {
            this.setNewJsonRpcProvider();
        }
        return {
            // eslint-disable-next-line @typescript-eslint/no-non-null-assertion
            fullNode: this._apiFullNodeProvider!,
        };
    }

    public getSignerInstance(keypair: Keypair): RawSigner {
        if (!this._apiFullNodeProvider) {
            this.setNewJsonRpcProvider();
        }
        if (!this._signer) {
            this._signer = new RawSigner(
                keypair,
                this._apiFullNodeProvider,

                growthbook.isOn(FEATURES.USE_LOCAL_TXN_SERIALIZER)
                    ? // eslint-disable-next-line @typescript-eslint/no-non-null-assertion
                      new LocalTxnDataSerializer(this._apiFullNodeProvider!)
                    : undefined
            );
        }
        return this._signer;
    }
}
