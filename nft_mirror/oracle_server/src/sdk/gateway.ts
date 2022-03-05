// Copyright (c) 2022, Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import { gatewayServiceAPI } from './gatewayServiceAPI';

/**
 * A connection to a Sui Gateway endpoint
 */
export class Connection {
    /** @internal */ _endpointURL: string;
    /** @internal */ _gatewayAPI;

    /**
     * Establish a connection to a Sui Gateway endpoint
     *
     * @param endpoint URL to the Sui Gateway endpoint
     */
    constructor(endpoint: string) {
        this._endpointURL = endpoint;
        this._gatewayAPI = gatewayServiceAPI({ baseUrl: endpoint });
    }

    /**
     * Retrieve all managed addresses for this client.
     */
    public async getAddresses(): Promise<string[]> {
        const {
            data: { addresses },
        } = await this._gatewayAPI.getAddresses({});
        return addresses;
    }
}
