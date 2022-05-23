// Copyright (c) 2022, Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

export enum Network {
    Local = 'Local',
    Devnet = 'Devnet',
}

const tryGetRpcParam = (): string | null => {
    const params = new URLSearchParams(window.location.search);
    return params.get('rpc');
};

const LOCALSTORE_RPC_KEY = 'sui-explorer-rpc';
const LOCALSTORE_RPC_TIME_KEY = 'sui-explorer-rpc-lastset';
const LOCALSTORE_RPC_VALID_MS = 60000 * 60 * 3;

// persisting this preference ad-hoc in local storage is to support localhost rpc
const tryGetRpcLocalStorage = (): string | null => {
    let value = window.localStorage.getItem(LOCALSTORE_RPC_KEY);
    const lastUpdated = window.localStorage.getItem(LOCALSTORE_RPC_TIME_KEY);

    if (lastUpdated) {
        const last = Number.parseInt(lastUpdated);
        const now = Date.now().valueOf();
        if (now === last) return value;

        const elapsed = now.valueOf() - last.valueOf();
        if (elapsed >= LOCALSTORE_RPC_VALID_MS) {
            window.localStorage.removeItem(LOCALSTORE_RPC_KEY);
            window.localStorage.removeItem(LOCALSTORE_RPC_TIME_KEY);
            value = null;
        }
    }

    return value;
};

export const tryGetRpcSetting = (): string | null => {
    const queryParam = tryGetRpcParam();
    const localStore = tryGetRpcLocalStorage();
    // query param takes precedence over local store
    return queryParam ? queryParam : localStore;
};

const ENDPOINTS = {
    [Network.Local]: 'http://127.0.0.1:5001',
    [Network.Devnet]: 'https://gateway.devnet.sui.io:443',
};

export function getEndpoint(network: Network | string): string {
    // Endpoint has 3 types:
    // 1) An override value
    const override = tryGetRpcSetting();
    if (override) return override;

    // 2) Default URLs to the Local RPC server and DevNet
    if (Object.keys(ENDPOINTS).includes(network)) {
        return ENDPOINTS[network as Network];
    }

    // 3) Custom URL provided by the user
    return network;
}
