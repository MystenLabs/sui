// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import { DEFAULT_API_ENV } from '_app/ApiProvider';
import { API_ENV } from '_src/shared/api-env';

import type { ObjectId, SuiAddress, TransactionDigest } from '@mysten/sui.js';

const API_ENV_TO_EXPLORER_ENV: Record<API_ENV, string | undefined> = {
    [API_ENV.local]: 'local',
    [API_ENV.devNet]: 'devnet',
    [API_ENV.testNet]: 'testnet',
    [API_ENV.mainnet]: 'mainnet',
    [API_ENV.customRPC]: '',
};

//TODO - this is a temporary solution, we should have a better way to get the explorer url
function getExplorerUrl(
    path: string,
    apiEnv: API_ENV = DEFAULT_API_ENV,
    customRPC: string
) {
    const base =
        apiEnv === API_ENV.local
            ? 'http://localhost:3000/'
            : 'https://explorer.sui.io/';

    const explorerEnv =
        apiEnv === 'customRPC' ? customRPC : API_ENV_TO_EXPLORER_ENV[apiEnv];

    const url = new URL(path, base);
    const searchParams = new URLSearchParams(url.search);
    if (explorerEnv) searchParams.set('network', explorerEnv);

    return url.href;
}

export function getObjectUrl(
    objectID: ObjectId,
    apiEnv: API_ENV,
    customRPC: string,
    moduleName?: string | null
) {
    return getExplorerUrl(
        `/object/${objectID}${moduleName ? `?module=${moduleName}` : ''}`,
        apiEnv,
        customRPC
    );
}

export function getTransactionUrl(
    txDigest: TransactionDigest,
    apiEnv: API_ENV,
    customRPC: string
) {
    return getExplorerUrl(
        `/txblock/${encodeURIComponent(txDigest)}`,
        apiEnv,
        customRPC
    );
}

export function getAddressUrl(
    address: SuiAddress,
    apiEnv: API_ENV,
    customRPC: string
) {
    return getExplorerUrl(`/address/${address}`, apiEnv, customRPC);
}

export function getValidatorUrl(
    address: SuiAddress,
    apiEnv: API_ENV,
    customRPC: string
) {
    return getExplorerUrl(`/validator/${address}`, apiEnv, customRPC);
}
