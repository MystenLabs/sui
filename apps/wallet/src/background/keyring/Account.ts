// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import type { Keypair, SuiAddress } from '@mysten/sui.js';

export type AccountType = 'derived' | 'imported';

export class Account {
    #keypair: Keypair;
    public readonly derivationPath: string | null;
    public readonly address: SuiAddress;

    constructor(
        type: 'derived',
        options: { derivationPath: string; keypair: Keypair }
    );
    constructor(type: 'imported', options: { keypair: Keypair });
    constructor(
        public readonly type: AccountType,
        options: { derivationPath?: string; keypair: Keypair }
    ) {
        this.derivationPath = options.derivationPath || null;
        this.#keypair = options.keypair;
        this.address = this.#keypair.getPublicKey().toSuiAddress();
    }

    exportKeypair() {
        return this.#keypair.export();
    }
}
