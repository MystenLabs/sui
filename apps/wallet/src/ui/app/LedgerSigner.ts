// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import {
    SignerWithProvider,
    Ed25519PublicKey,
    Base64DataBuffer,
} from '@mysten/sui.js';

import WebHIDTransport from '@ledgerhq/hw-transport-webhid';
import WebUSBTransport from '@ledgerhq/hw-transport-webusb';
import type Transport from '@ledgerhq/hw-transport';
import AppSui from 'hw-app-sui';
import type {
    Provider,
    SignaturePubkeyPair,
    SuiAddress,
    TxnDataSerializer,
} from '@mysten/sui.js';

export class LedgerSigner extends SignerWithProvider {
    readonly appSui: Promise<AppSui>;
    readonly bip32path = "44'/784'/0'/0/0";
    constructor(
        appSui: Promise<AppSui>,
        provider?: Provider,
        serializer?: TxnDataSerializer
    ) {
        super(provider, serializer);
        this.appSui = appSui;
    }

    static create(
        provider?: Provider,
        serializer?: TxnDataSerializer
    ): LedgerSigner {
        const getTransport = async () => {
            let transport = null;
            let error;
            try {
                transport = await WebHIDTransport.request();
            } catch (e) {
                console.error(`HID Transport is not supported: ${e}`);
                error = e;
            }

            if ((window as any).USB) {
                try {
                    transport = await WebUSBTransport.request();
                } catch (e) {
                    console.error(`WebUSB Transport is not supported: ${e}`);
                    error = e;
                }
            }
            if (transport != null) {
                return transport;
            } else {
                throw error;
            }
        };
        return new LedgerSigner(
            (async () => new AppSui(await getTransport()))(),
            provider,
            serializer
        );
    }

    async getAddress(): Promise<string> {
        return (
            (await (await this.appSui).getPublicKey(this.bip32path)).address ||
            ''
        );
    }

    async getPublicKey(): Promise<Ed25519PublicKey> {
        const { publicKey } = await (
            await this.appSui
        ).getPublicKey(this.bip32path);
        return new Ed25519PublicKey(
            Uint8Array.from(Buffer.from(publicKey, 'hex'))
        );
    }

    async signData(data: Base64DataBuffer): Promise<SignaturePubkeyPair> {
        const { signature } = await (
            await this.appSui
        ).signTransaction(this.bip32path, data.getData());
        return {
            signatureScheme: 'ED25519',
            signature: new Base64DataBuffer(
                Uint8Array.from(Buffer.from(signature, 'hex'))
            ),
            pubKey: await this.getPublicKey(),
        };
    }

    connect(provider: Provider): SignerWithProvider {
        return new LedgerSigner(this.appSui, provider);
    }
}
