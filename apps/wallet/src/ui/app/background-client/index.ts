// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import { lastValueFrom, map, take } from 'rxjs';

import { createMessage } from '_messages';
import { PortStream } from '_messaging/PortStream';
import { isKeyringPayload } from '_payloads/keyring';
import { isPermissionRequests } from '_payloads/permissions';
import { isUpdateActiveOrigin } from '_payloads/tabs/updateActiveOrigin';
import { isGetTransactionRequestsResponse } from '_payloads/transactions/ui/GetTransactionRequestsResponse';
import { setActiveOrigin } from '_redux/slices/app';
import { setPermissions } from '_redux/slices/permissions';
import { setTransactionRequests } from '_redux/slices/transaction-requests';

import type { SuiAddress, SuiTransactionResponse } from '@mysten/sui.js';
import type { Message } from '_messages';
import type { KeyringPayload } from '_payloads/keyring';
import type {
    GetPermissionRequests,
    PermissionResponse,
} from '_payloads/permissions';
import type { DisconnectApp } from '_payloads/permissions/DisconnectApp';
import type { GetTransactionRequests } from '_payloads/transactions/ui/GetTransactionRequests';
import type { TransactionRequestResponse } from '_payloads/transactions/ui/TransactionRequestResponse';
import type { AppDispatch } from '_store';

export class BackgroundClient {
    private _portStream: PortStream | null = null;
    private _dispatch: AppDispatch | null = null;
    private _initialized = false;

    public async init(dispatch: AppDispatch) {
        if (this._initialized) {
            throw new Error('[BackgroundClient] already initialized');
        }
        this._initialized = true;
        this._dispatch = dispatch;
        this.createPortStream();
        return Promise.all([
            this.sendGetPermissionRequests(),
            this.sendGetTransactionRequests(),
        ]).then(() => undefined);
    }

    public sendPermissionResponse(
        id: string,
        accounts: SuiAddress[],
        allowed: boolean,
        responseDate: string
    ) {
        this.sendMessage(
            createMessage<PermissionResponse>({
                id,
                type: 'permission-response',
                accounts,
                allowed,
                responseDate,
            })
        );
    }

    public async sendGetPermissionRequests() {
        return lastValueFrom(
            this.sendMessage(
                createMessage<GetPermissionRequests>({
                    type: 'get-permission-requests',
                })
            ).pipe(take(1))
        );
    }

    public async sendTransactionRequestResponse(
        txID: string,
        approved: boolean,
        txResult: SuiTransactionResponse | undefined,
        tsResultError: string | undefined
    ) {
        this.sendMessage(
            createMessage<TransactionRequestResponse>({
                type: 'transaction-request-response',
                approved,
                txID,
                txResult,
                tsResultError,
            })
        );
    }

    public async sendGetTransactionRequests() {
        return lastValueFrom(
            this.sendMessage(
                createMessage<GetTransactionRequests>({
                    type: 'get-transaction-requests',
                })
            ).pipe(take(1))
        );
    }

    public async disconnectApp(origin: string) {
        await lastValueFrom(
            this.sendMessage(
                createMessage<DisconnectApp>({ type: 'disconnect-app', origin })
            ).pipe(take(1))
        );
    }

    // TODO: password should be required (#encrypt-wallet)
    public async createMnemonic(password?: string, importedMnemonic?: string) {
        return await lastValueFrom(
            this.sendMessage(
                createMessage<KeyringPayload<'createMnemonic'>>({
                    type: 'keyring',
                    method: 'createMnemonic',
                    args: { password: password || '', importedMnemonic },
                    return: undefined,
                })
            ).pipe(take(1))
        );
    }

    public async unlockWallet(password: string) {
        return await lastValueFrom(
            this.sendMessage(
                createMessage<KeyringPayload<'unlock'>>({
                    type: 'keyring',
                    method: 'unlock',
                    args: { password: password },
                    return: undefined,
                })
            ).pipe(take(1))
        );
    }

    public async getMnemonic(password?: string) {
        return await lastValueFrom(
            this.sendMessage(
                createMessage<KeyringPayload<'getMnemonic'>>({
                    type: 'keyring',
                    method: 'getMnemonic',
                    args: password,
                    return: undefined,
                })
            ).pipe(
                take(1),
                map(({ payload }) => {
                    if (
                        isKeyringPayload<'getMnemonic'>(
                            payload,
                            'getMnemonic'
                        ) &&
                        payload.return
                    ) {
                        return payload.return;
                    }
                    throw new Error('Mnemonic not found');
                })
            )
        );
    }

    private handleIncomingMessage(msg: Message) {
        if (!this._initialized || !this._dispatch) {
            throw new Error(
                'BackgroundClient is not initialized to handle incoming messages'
            );
        }
        const { payload } = msg;
        let action;
        if (isPermissionRequests(payload)) {
            action = setPermissions(payload.permissions);
        } else if (isGetTransactionRequestsResponse(payload)) {
            action = setTransactionRequests(payload.txRequests);
        } else if (isUpdateActiveOrigin(payload)) {
            action = setActiveOrigin(payload);
        }
        if (action) {
            this._dispatch(action);
        }
    }

    private createPortStream() {
        this._portStream = PortStream.connectToBackgroundService(
            'sui_ui<->background'
        );
        this._portStream.onDisconnect.subscribe(() => {
            this.createPortStream();
        });
        this._portStream.onMessage.subscribe((msg) =>
            this.handleIncomingMessage(msg)
        );
    }

    private sendMessage(msg: Message) {
        if (this._portStream?.connected) {
            return this._portStream.sendMessage(msg);
        } else {
            throw new Error(
                'Failed to send message to background service. Port not connected.'
            );
        }
    }
}
