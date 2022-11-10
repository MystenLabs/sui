// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import { API_ENV, DEFAULT_API_ENV } from '_app/ApiProvider';

import type { ObjectId, SuiAddress, TransactionDigest } from '@mysten/sui.js';

const API_ENV_TO_EXPLORER_URL: Record<API_ENV, string | undefined> = {
    [API_ENV.local]: process.env.EXPLORER_URL_LOCAL,
    [API_ENV.devNet]: process.env.EXPLORER_URL_DEV_NET,
    [API_ENV.staging]: process.env.EXPLORER_URL_STAGING,
    [API_ENV.testNet]: process.env.EXPLORER_URL_TEST_NET,
    //No explorer url for Custom PRC
    [API_ENV.customRPC]: '',
};

// TODO: rewrite this
function getDefaultUrl(apiEnv?: API_ENV) {
    const url = API_ENV_TO_EXPLORER_URL[apiEnv || DEFAULT_API_ENV];
    if (!url && !API_ENV.customRPC) {
        throw new Error(`Url for API_ENV ${DEFAULT_API_ENV} is not defined`);
    }
    return url;
}

export class Explorer {
    public static getObjectUrl(objectID: ObjectId, apiEnv: API_ENV) {
        return new URL(`/objects/${objectID}`, getDefaultUrl(apiEnv)).href;
    }

    public static getTransactionUrl(
        txDigest: TransactionDigest,
        apiEnv: API_ENV
    ) {
        return new URL(
            `/transactions/${encodeURIComponent(txDigest)}`,
            getDefaultUrl(apiEnv)
        ).href;
    }

    public static getAddressUrl(address: SuiAddress, apiEnv: API_ENV) {
        return new URL(`/addresses/${address}`, getDefaultUrl(apiEnv)).href;
    }
}
