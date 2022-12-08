// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import { fromExportedKeypair } from '@mysten/sui.js';

import type { Keypair, ExportedKeypair } from '@mysten/sui.js';

export default class KeypairVault {
    private _keypair: Keypair | null = null;

    public set keypair(keypair: ExportedKeypair) {
        this._keypair = fromExportedKeypair(keypair);
    }

    public getAccount(): string | null {
        let address = this._keypair?.getPublicKey().toSuiAddress() || null;
        if (address && !address.startsWith('0x')) {
            address = `0x${address}`;
        }
        return address;
    }

    public getKeypair() {
        if (!this._keypair) {
            throw new Error('Account keypair is not set');
        }
        return this._keypair;
    }

    public clear() {
        this._keypair = null;
    }
}
