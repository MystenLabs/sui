// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import { SentryRpcClient } from '@mysten/core';
import { Connection, JsonRpcProvider } from '@mysten/sui.js';

import { LedgerSigner } from './LedgerSigner';
import { BackgroundServiceSigner } from './background-client/BackgroundServiceSigner';
import { queryClient } from './helpers/queryClient';
import { growthbook } from '_app/experimentation/feature-gating';
import {
    AccountType,
    type SerializedAccount,
} from '_src/background/keyring/Account';
import { API_ENV } from '_src/shared/api-env';
import { FEATURES } from '_src/shared/experimentation/features';

import type { BackgroundClient } from './background-client';
import type SuiLedgerClient from '@mysten/ledgerjs-hw-app-sui';
import type { SuiAddress, SignerWithProvider } from '@mysten/sui.js';

type EnvInfo = {
    name: string;
    env: API_ENV;
};

export const API_ENV_TO_INFO: Record<API_ENV, EnvInfo> = {
    [API_ENV.local]: { name: 'Local', env: API_ENV.local },
    [API_ENV.devNet]: { name: 'Sui Devnet', env: API_ENV.devNet },
    [API_ENV.customRPC]: { name: 'Custom RPC URL', env: API_ENV.customRPC },
    [API_ENV.testNet]: { name: 'Sui Testnet', env: API_ENV.testNet },
};

export const ENV_TO_API: Record<API_ENV, Connection | null> = {
    [API_ENV.local]: new Connection({
        fullnode: process.env.API_ENDPOINT_LOCAL_FULLNODE || '',
        faucet: process.env.API_ENDPOINT_LOCAL_FAUCET || '',
    }),
    [API_ENV.devNet]: new Connection({
        fullnode: process.env.API_ENDPOINT_DEV_NET_FULLNODE || '',
        faucet: process.env.API_ENDPOINT_DEV_NET_FAUCET || '',
    }),
    [API_ENV.customRPC]: null,
    [API_ENV.testNet]: new Connection({
        fullnode: process.env.API_ENDPOINT_TEST_NET_FULLNODE || '',
        // NOTE: Faucet is currently disabled for testnet:
        // faucet: process.env.API_ENDPOINT_TEST_NET_FAUCET || '',
    }),
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
        apiEndpoint.fullnode === '' ||
        apiEndpoint.faucet === ''
    ) {
        throw new Error(`API endpoint not found for API_ENV ${env}`);
    }
    return apiEndpoint;
}

export const DEFAULT_API_ENV = getDefaultApiEnv();
const SENTRY_MONITORED_ENVS = [API_ENV.devNet, API_ENV.testNet];

type NetworkTypes = keyof typeof API_ENV;

export const generateActiveNetworkList = (): NetworkTypes[] => {
    const excludedNetworks: NetworkTypes[] = [];

    if (!growthbook.isOn(FEATURES.USE_TEST_NET_ENDPOINT)) {
        excludedNetworks.push(API_ENV.testNet);
    }

    return Object.values(API_ENV).filter(
        (env) => !excludedNetworks.includes(env as keyof typeof API_ENV)
    );
};

export default class ApiProvider {
    private _apiFullNodeProvider?: JsonRpcProvider;
    private _softwareSignerByAddress: Map<SuiAddress, SignerWithProvider> =
        new Map();

    public setNewJsonRpcProvider(
        apiEnv: API_ENV = DEFAULT_API_ENV,
        customRPC?: string | null
    ) {
        const connection = customRPC
            ? new Connection({ fullnode: customRPC })
            : getDefaultAPI(apiEnv);
        this._apiFullNodeProvider = new JsonRpcProvider(connection, {
            rpcClient:
                !customRPC && SENTRY_MONITORED_ENVS.includes(apiEnv)
                    ? new SentryRpcClient(connection.fullnode)
                    : undefined,
        });

        this._softwareSignerByAddress.clear();

        // We also clear the query client whenever set set a new API provider:
        queryClient.resetQueries();
        queryClient.clear();
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

    public getSignerInstance(
        account: SerializedAccount,
        backgroundClient: BackgroundClient
    ): SignerWithProvider {
        if (!this._apiFullNodeProvider) {
            this.setNewJsonRpcProvider();
        }

        return this.getSoftwareSignerInstance(
            account.address,
            backgroundClient
        );
    }

    public getSoftwareSignerInstance(
        address: SuiAddress,
        backgroundClient: BackgroundClient
    ): SignerWithProvider {
        if (!this._softwareSignerByAddress.has(address)) {
            this._softwareSignerByAddress.set(
                address,
                new BackgroundServiceSigner(
                    address,
                    backgroundClient,
                    this._apiFullNodeProvider!
                )
            );
        }
        return this._softwareSignerByAddress.get(address)!;
    }
}
