// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import { SignerWithProvider } from '@mysten/sui.js';

import Transport from '@ledgerhq/hw-transport';
import AppSui from 'hw-app-sui';
import type {
    Base64DataBuffer,
    Provider,
    SignaturePubkeyPair,
    SuiAddress,
    TxnDataSerializer,
} from '@mysten/sui.js';

export class LedgerSigner extends SignerWithProvider {
    readonly #appSui: AppSui;
    bip32path = "44'/784'/0'/0/0";
    constructor(provider?: Provider, serializer?: TxnDataSerializer) {
        super(provider, serializer);
        this.#appSui = new AppSui(await Transport.create());
    }

    async getAddress(): Promise<string> {
        return await this.#appSui.getPublicKey(this.bip32path);
    }

    signData(data: Base64DataBuffer): Promise<SignaturePubkeyPair> {
        return this.#appSui.signTransaction(this.bip32path, data.getData());
    }

    connect(provider: Provider): SignerWithProvider {
        return new LedgerSigner(provider);
    }
}
