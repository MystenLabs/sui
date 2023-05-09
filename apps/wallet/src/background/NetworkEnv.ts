// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import mitt from 'mitt';
import Browser from 'webextension-polyfill';

import { API_ENV_TO_INFO, DEFAULT_API_ENV } from '_app/ApiProvider';
import { API_ENV } from '_src/shared/api-env';
import { FEATURES, growthbook } from '_src/shared/experimentation/features';
import { isValidUrl } from '_src/shared/utils';

export type NetworkEnvType =
    | { env: Exclude<API_ENV, API_ENV.customRPC>; customRpcUrl: null }
    | { env: API_ENV.customRPC; customRpcUrl: string };

class NetworkEnv {
    #events = mitt<{ changed: NetworkEnvType }>();

    async getActiveNetwork(): Promise<NetworkEnvType> {
        const { sui_Env, sui_Env_RPC } = await Browser.storage.local.get({
            sui_Env: DEFAULT_API_ENV,
            sui_Env_RPC: null,
        });
        const adjEnv = (await this.#isNetworkAvailable(sui_Env))
            ? sui_Env
            : DEFAULT_API_ENV;
        const adjCustomUrl = adjEnv === API_ENV.customRPC ? sui_Env_RPC : null;
        return { env: adjEnv, customRpcUrl: adjCustomUrl };
    }

    async setActiveNetwork(network: NetworkEnvType) {
        const { env, customRpcUrl } = network;
        if (!(await this.#isNetworkAvailable(env))) {
            throw new Error(
                `Error changing network, ${API_ENV_TO_INFO[env].name} is not available.`
            );
        }
        if (env === API_ENV.customRPC && !isValidUrl(customRpcUrl)) {
            throw new Error(`Invalid custom RPC url ${customRpcUrl}`);
        }
        await Browser.storage.local.set({
            sui_Env: env,
            sui_Env_RPC: customRpcUrl,
        });
        this.#events.emit('changed', network);
    }

    on = this.#events.on;

    off = this.#events.off;

    async #isNetworkAvailable(apiEnv: API_ENV) {
        await growthbook.loadFeatures();
        return (
            (apiEnv === API_ENV.mainnet &&
                growthbook.isOn(FEATURES.USE_MAINNET_ENDPOINT)) ||
            (apiEnv === API_ENV.testNet &&
                growthbook.isOn(FEATURES.USE_TEST_NET_ENDPOINT)) ||
            ![API_ENV.testNet, API_ENV.mainnet].includes(apiEnv)
        );
    }
}

export default new NetworkEnv();
