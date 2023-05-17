// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import {
    type SerializedSignature,
    SignerWithProvider,
    type SuiAddress,
    type JsonRpcProvider,
    toB64,
    TransactionBlock,
    IntentScope,
    messageWithIntent,
} from '@mysten/sui.js';
import mitt from 'mitt';

import { type SerializedQredoAccount } from '_src/background/keyring/QredoAccount';
import { type QredoAPI } from '_src/shared/qredo-api';

function wait(milliseconds = 1000) {
    return new Promise((r) => setTimeout(r, milliseconds));
}

export class QredoSigner extends SignerWithProvider {
    #qredoAccount: SerializedQredoAccount;
    #qredoAPI: QredoAPI;
    #events = mitt();

    constructor(
        provider: JsonRpcProvider,
        account: SerializedQredoAccount,
        qredoAPI: QredoAPI
    ) {
        super(provider);
        this.#qredoAccount = account;
        this.#qredoAPI = qredoAPI;
    }

    async getAddress(): Promise<SuiAddress> {
        return this.#qredoAccount.address;
    }

    async signData(data: Uint8Array): Promise<SerializedSignature> {
        let txInfo = await this.#qredoAPI.createTransaction({
            messageWithIntent: toB64(data),
            network: 'devnet', // TODO use correct network
            broadcast: false,
            from: await this.getAddress(),
        });
        while (
            !txInfo.sig &&
            ['pending', 'created', 'authorized', 'approved', 'signed'].includes(
                txInfo.status
            )
        ) {
            await wait(4000);
            txInfo = await this.#qredoAPI.getTransaction(txInfo.txID);
        }
        if (
            ['cancelled', 'expired', 'failed', 'rejected'].includes(
                txInfo.status
            )
        ) {
            throw new Error(`Transaction ${txInfo.status}`);
        }
        if (!txInfo.sig) {
            throw new Error('Failed to retrieve signature');
        }
        return txInfo.sig;
    }

    signAndExecuteTransactionBlock: SignerWithProvider['signAndExecuteTransactionBlock'] =
        async ({ transactionBlock, options }) => {
            const transactionBytes = await this.prepareTransactionBlock(
                transactionBlock
            );
            const intent = messageWithIntent(
                IntentScope.TransactionData,
                transactionBytes
            );
            let txInfo = await this.#qredoAPI.createTransaction({
                messageWithIntent: toB64(intent),
                network: 'testnet', // TODO use correct network
                broadcast: true,
                from: await this.getAddress(),
            });
            while (
                [
                    'pending',
                    'created',
                    'authorized',
                    'approved',
                    'signed',
                    'scheduled',
                    'queued',
                    'pushed',
                ].includes(txInfo.status)
            ) {
                await wait(4000);
                txInfo = await this.#qredoAPI.getTransaction(txInfo.txID);
            }
            const theTransactionBlock = TransactionBlock.is(transactionBlock)
                ? transactionBlock
                : TransactionBlock.from(transactionBlock);
            return this.provider.getTransactionBlock({
                digest: await theTransactionBlock.getDigest({
                    provider: this.provider,
                }),
                options,
            });
        };

    connect(provider: JsonRpcProvider): SignerWithProvider {
        return new QredoSigner(provider, this.#qredoAccount, this.#qredoAPI);
    }
}
