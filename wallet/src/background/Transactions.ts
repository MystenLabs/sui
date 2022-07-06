// Copyright (c) 2022, Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import { filter, lastValueFrom, map, race, Subject, take } from 'rxjs';
import { v4 as uuidV4 } from 'uuid';
import Browser from 'webextension-polyfill';

import { Window } from './Window';
import { Base64DataBuffer } from '@mysten/sui.js';
import type { TransactionBytesRequest, TransactionRequest } from '_payloads/transactions';
import type { TransactionRequestResponse } from '_payloads/transactions/ui/TransactionRequestResponse';
import type { ContentScriptConnection } from '_src/background/connections/ContentScriptConnection';

type Transaction = TransactionRequest['tx'];

const TX_STORE_KEY = 'transactions';
const TX_BYTES_STORE_KEY = 'transactions-bytes';

function openTxWindow(txRequestID: string) {
    return new Window(
        Browser.runtime.getURL('ui.html') +
            `#/tx-approval/${encodeURIComponent(txRequestID)}`
    );
}

class Transactions {
    private _txResponseMessages = new Subject<TransactionRequestResponse>();

    public async executeTransaction(
        tx: Transaction,
        connection: ContentScriptConnection
    ) {
        const txRequest = this.createTransactionRequest(
            tx,
            connection.origin,
            connection.originFavIcon
        );
        await this.storeTransactionRequest(txRequest);
        const popUp = openTxWindow(txRequest.id);
        const popUpClose = (await popUp.show()).pipe(
            take(1),
            map<number, false>(() => false)
        );
        const txResponseMessage = this._txResponseMessages.pipe(
            filter((msg) => msg.txID === txRequest.id),
            take(1)
        );
        return lastValueFrom(
            race(popUpClose, txResponseMessage).pipe(
                take(1),
                map(async (response) => {
                    if (response) {
                        const { approved, txResult, tsResultError } = response;
                        if (approved) {
                            txRequest.approved = approved;
                            txRequest.txResult = txResult;
                            txRequest.txResultError = tsResultError;
                            await this.storeTransactionRequest(txRequest);
                            if (tsResultError) {
                                throw new Error(
                                    `Transaction failed with the following error. ${tsResultError}`
                                );
                            }
                            if (!txResult) {
                                throw new Error(`Transaction result is empty`);
                            }
                            return txResult;
                        }
                    }
                    await this.removeTransactionRequest(txRequest.id);
                    throw new Error('Transaction rejected from user');
                })
            )
        );
    }

    public async executeTransactionBytes(
        txBytes: Base64DataBuffer,
        connection: ContentScriptConnection
    ) {
        const txBytesRequest = this.createTransactionBytesRequest(
            txBytes,
            connection.origin,
            connection.originFavIcon
        );
        await this.storeTransactionBytesRequest(txBytesRequest);
        // TODO(ggao), need to pop up a different window?
        const popUp = openTxWindow(txBytesRequest.id);
        const popUpClose = (await popUp.show()).pipe(
            take(1),
            map<number, false>(() => false)
        );
        const txResponseMessage = this._txResponseMessages.pipe(
            filter((msg) => msg.txID === txBytesRequest.id),
            take(1)
        );
        return lastValueFrom(
            race(popUpClose, txResponseMessage).pipe(
                take(1),
                map(async (response) => {
                    if (response) {
                        const { approved, txResult, tsResultError } = response;
                        if (approved) {
                            txBytesRequest.approved = approved;
                            txBytesRequest.txResult = txResult;
                            txBytesRequest.txResultError = tsResultError;
                            await this.storeTransactionBytesRequest(txBytesRequest);
                            if (tsResultError) {
                                throw new Error(
                                    `Serialized transaction failed with the following error. ${tsResultError}`
                                );
                            }
                            if (!txResult) {
                                throw new Error(`Serialized transaction result is empty`);
                            }
                            return txResult;
                        }
                    }
                    await this.removeTransactionBytesRequest(txBytesRequest.id);
                    throw new Error('Serialized transaction rejected from user');
                })
            )
        );
    }

    public async getTransactionRequests(): Promise<
        Record<string, TransactionRequest>
    > {
        return (await Browser.storage.local.get({ [TX_STORE_KEY]: {} }))[
            TX_STORE_KEY
        ];
    }

    public async getTransactionRequest(
        txRequestID: string
    ): Promise<TransactionRequest | null> {
        return (await this.getTransactionRequests())[txRequestID] || null;
    }

    public handleMessage(msg: TransactionRequestResponse) {
        this._txResponseMessages.next(msg);
    }

    private createTransactionRequest(
        tx: Transaction,
        origin: string,
        originFavIcon?: string
    ): TransactionRequest {
        return {
            id: uuidV4(),
            tx,
            approved: null,
            origin,
            originFavIcon,
            createdDate: new Date().toISOString(),
        };
    }

    private async saveTransactionRequests(
        txRequests: Record<string, TransactionRequest>
    ) {
        await Browser.storage.local.set({ [TX_STORE_KEY]: txRequests });
    }

    private async storeTransactionRequest(txRequest: TransactionRequest) {
        const txs = await this.getTransactionRequests();
        txs[txRequest.id] = txRequest;
        await this.saveTransactionRequests(txs);
    }

    private async removeTransactionRequest(txID: string) {
        const txs = await this.getTransactionRequests();
        delete txs[txID];
        await this.saveTransactionRequests(txs);
    }

    private createTransactionBytesRequest(
        txBytes: Base64DataBuffer,
        origin: string,
        originFavIcon?: string
    ): TransactionBytesRequest {
        return {
            id: uuidV4(),
            txBytes,
            approved: null,
            origin,
            originFavIcon,
            createdDate: new Date().toISOString(),
        };
    }

    private async saveTransactionBytesRequests(
        txBytesRequests: Record<string, TransactionBytesRequest>
    ) {
        await Browser.storage.local.set({ [TX_BYTES_STORE_KEY]: txBytesRequests });
    }

    public async getTransactionBytesRequests(): Promise<
        Record<string, TransactionBytesRequest>
    > {
        return (await Browser.storage.local.get({ [TX_BYTES_STORE_KEY]: {} }))[
            TX_BYTES_STORE_KEY
        ];
    }

    private async storeTransactionBytesRequest(txBytesRequest: TransactionBytesRequest) {
        const tx_bytes_requests = await this.getTransactionBytesRequests();
        tx_bytes_requests[txBytesRequest.id] = txBytesRequest;
        await this.saveTransactionBytesRequests(tx_bytes_requests);
    }

    private async removeTransactionBytesRequest(txBytesID: string) {
        const tx_bytes_requests = await this.getTransactionBytesRequests();
        delete tx_bytes_requests[txBytesID];
        await this.saveTransactionBytesRequests(tx_bytes_requests);
    }
}

export default new Transactions();
