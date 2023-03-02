// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import type Transport from '@ledgerhq/hw-transport';
import { Common } from './Common';
import type { SignTransactionResult, GetVersionResult } from './Common';

export type { SignTransactionResult, GetVersionResult };

export type GetPublicKeyResult = {
    publicKey: Uint8Array;
    address: Uint8Array;
};

/**
 * Sui API
 *
 * @example
 * import Sui from "hw-app-sui";
 * const sui = new Sui(transport)
 */
export default class Sui extends Common {
    constructor(transport: Transport) {
        super(transport, 'SUI');
        this.sendChunks = this.sendWithBlocks;
    }

    /**
     * Retrieves the public key associated with a particular BIP32 path from the ledger app.
     *
     * @param path - the path to retrieve.
     */
    override async getPublicKey(path: string): Promise<GetPublicKeyResult> {
        const { publicKey, address } = await super.getPublicKey(path);
        if (address == null) {
            throw new Error('should never happen, app always returns address');
        }
        return { publicKey, address };
    }
}
