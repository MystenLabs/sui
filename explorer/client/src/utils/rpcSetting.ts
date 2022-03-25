// Copyright (c) 2022, Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

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
