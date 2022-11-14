// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import { API_ENV, DEFAULT_API_ENV } from '_app/ApiProvider';

import type { ObjectId, SuiAddress, TransactionDigest } from '@mysten/sui.js';

const API_ENV_TO_EXPLORER_ENV: Record<API_ENV, string | undefined> = {
    [API_ENV.local]: 'local',
    [API_ENV.devNet]: 'devnet',
    [API_ENV.staging]: 'staging',
    [API_ENV.testNet]: 'testnet',
    [API_ENV.customRPC]: '',
};

function getExplorerUrl(path: string, apiEnv: API_ENV = DEFAULT_API_ENV) {
    const base =
        apiEnv === API_ENV.local
            ? 'http://localhost:3000/'
            : 'https://explorer.sui.io/';

    return new URL(`${path}/?network=${API_ENV_TO_EXPLORER_ENV[apiEnv]}`, base)
        .href;
}

export function getObjectUrl(objectID: ObjectId, apiEnv: API_ENV) {
    return getExplorerUrl(`/objects/${objectID}`, apiEnv);
}

export function getTransactionUrl(
    txDigest: TransactionDigest,
    apiEnv: API_ENV
) {
    return getExplorerUrl(
        `/transactions/${encodeURIComponent(txDigest)}`,
        apiEnv
    );
}

export function getAddressUrl(address: SuiAddress, apiEnv: API_ENV) {
    return getExplorerUrl(`/addresses/${address}`, apiEnv);
}
