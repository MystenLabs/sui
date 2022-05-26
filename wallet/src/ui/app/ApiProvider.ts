// Copyright (c) 2022, Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import { JsonRpcProvider } from '@mysten/sui.js';

// const DEFAULT_API_ENDPOINT = 'https://gateway.devnet.sui.io';
const DEFAULT_API_ENDPOINT = 'http://127.0.0.1:5001';

export default class ApiProvider {
    private _apiProvider: JsonRpcProvider;

    constructor() {
        // TODO: allow overriding default endpoint
        const apiEndpoint = DEFAULT_API_ENDPOINT;
        this._apiProvider = new JsonRpcProvider(apiEndpoint);
    }

    public get instance() {
        return this._apiProvider;
    }
}
