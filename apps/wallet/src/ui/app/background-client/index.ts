// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import {
    type SerializedSignature,
    toB64,
    type SignedTransaction,
    type ExportedKeypair,
    type SignedMessage,
} from '@mysten/sui.js';
import { lastValueFrom, map, take } from 'rxjs';

import { growthbook } from '../experimentation/feature-gating';
import { createMessage } from '_messages';
import { PortStream } from '_messaging/PortStream';
import { type BasePayload } from '_payloads';
import { isLoadedFeaturesPayload } from '_payloads/feature-gating';
import { isKeyringPayload } from '_payloads/keyring';
import { isSetNetworkPayload, type SetNetworkPayload } from '_payloads/network';
import { isPermissionRequests } from '_payloads/permissions';
import { isUpdateActiveOrigin } from '_payloads/tabs/updateActiveOrigin';
import { isGetTransactionRequestsResponse } from '_payloads/transactions/ui/GetTransactionRequestsResponse';
import { setKeyringStatus } from '_redux/slices/account';
import { setActiveOrigin, changeActiveNetwork } from '_redux/slices/app';
import { setPermissions } from '_redux/slices/permissions';
import { setTransactionRequests } from '_redux/slices/transaction-requests';
import { type SerializedLedgerAccount } from '_src/background/keyring/LedgerAccount';

import type { SuiAddress, SuiTransactionBlockResponse } from '@mysten/sui.js';
import type { Message } from '_messages';
import type { KeyringPayload } from '_payloads/keyring';
import type {
    GetPermissionRequests,
    PermissionResponse,
} from '_payloads/permissions';
import type { DisconnectApp } from '_payloads/permissions/DisconnectApp';
import type { GetTransactionRequests } from '_payloads/transactions/ui/GetTransactionRequests';
import type { TransactionRequestResponse } from '_payloads/transactions/ui/TransactionRequestResponse';
import type { NetworkEnvType } from '_src/background/NetworkEnv';
import type { AppDispatch } from '_store';

/**
 * The duration in milliseconds that the UI sends status updates (active/inactive) to the background service.
 * Currently used to postpone auto locking keyring when the app is active.
 */
const APP_STATUS_UPDATE_INTERVAL = 20 * 1000;

export class BackgroundClient {
    private _portStream: PortStream | null = null;
    private _dispatch: AppDispatch | null = null;
    private _initialized = false;

    public init(dispatch: AppDispatch) {
        if (this._initialized) {
            throw new Error('[BackgroundClient] already initialized');
        }
        this._initialized = true;
        this._dispatch = dispatch;
        this.createPortStream();
        this.sendAppStatus();
        this.setupAppStatusUpdateInterval();
        return Promise.all([
            this.sendGetPermissionRequests(),
            this.sendGetTransactionRequests(),
            this.getWalletStatus(),
            this.loadFeatures(),
            this.getNetwork(),
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

    public sendGetPermissionRequests() {
        return lastValueFrom(
            this.sendMessage(
                createMessage<GetPermissionRequests>({
                    type: 'get-permission-requests',
                })
            ).pipe(take(1))
        );
    }

    public sendTransactionRequestResponse(
        txID: string,
        approved: boolean,
        txResult?: SuiTransactionBlockResponse | SignedMessage,
        txResultError?: string,
        txSigned?: SignedTransaction
    ) {
        this.sendMessage(
            createMessage<TransactionRequestResponse>({
                type: 'transaction-request-response',
                approved,
                txID,
                txResult,
                txResultError,
                txSigned,
            })
        );
    }

    public sendGetTransactionRequests() {
        return lastValueFrom(
            this.sendMessage(
                createMessage<GetTransactionRequests>({
                    type: 'get-transaction-requests',
                })
            ).pipe(take(1))
        );
    }

    /**
     * Disconnect a dapp, if specificAccounts contains accounts then only those accounts will be disconnected.
     * @param origin The origin of the dapp
     * @param specificAccounts Accounts to disconnect. If not provided or it's an empty array all accounts will be disconnected
     */
    public async disconnectApp(
        origin: string,
        specificAccounts?: SuiAddress[]
    ) {
        await lastValueFrom(
            this.sendMessage(
                createMessage<DisconnectApp>({
                    type: 'disconnect-app',
                    origin,
                    specificAccounts,
                })
            ).pipe(take(1))
        );
    }

    public createVault(password: string, importedEntropy?: string) {
        return lastValueFrom(
            this.sendMessage(
                createMessage<KeyringPayload<'create'>>({
                    type: 'keyring',
                    method: 'create',
                    args: { password, importedEntropy },
                    return: undefined,
                })
            ).pipe(take(1))
        );
    }

    public unlockWallet(password: string) {
        return lastValueFrom(
            this.sendMessage(
                createMessage<KeyringPayload<'unlock'>>({
                    type: 'keyring',
                    method: 'unlock',
                    args: { password },
                    return: undefined,
                })
            ).pipe(take(1))
        );
    }

    public lockWallet() {
        return lastValueFrom(
            this.sendMessage(
                createMessage<KeyringPayload<'lock'>>({
                    type: 'keyring',
                    method: 'lock',
                })
            ).pipe(take(1))
        );
    }

    public clearWallet() {
        return lastValueFrom(
            this.sendMessage(
                createMessage<KeyringPayload<'clear'>>({
                    type: 'keyring',
                    method: 'clear',
                })
            ).pipe(take(1))
        );
    }

    public getEntropy(password?: string) {
        return lastValueFrom(
            this.sendMessage(
                createMessage<KeyringPayload<'getEntropy'>>({
                    type: 'keyring',
                    method: 'getEntropy',
                    args: password,
                    return: undefined,
                })
            ).pipe(
                take(1),
                map(({ payload }) => {
                    if (
                        isKeyringPayload(payload, 'getEntropy') &&
                        payload.return
                    ) {
                        return payload.return;
                    }
                    throw new Error('Mnemonic not found');
                })
            )
        );
    }

    public setKeyringLockTimeout(timeout: number) {
        return lastValueFrom(
            this.sendMessage(
                createMessage<KeyringPayload<'setLockTimeout'>>({
                    type: 'keyring',
                    method: 'setLockTimeout',
                    args: { timeout },
                })
            ).pipe(take(1))
        );
    }

    public signData(
        address: SuiAddress,
        data: Uint8Array
    ): Promise<SerializedSignature> {
        return lastValueFrom(
            this.sendMessage(
                createMessage<KeyringPayload<'signData'>>({
                    type: 'keyring',
                    method: 'signData',
                    args: { data: toB64(data), address },
                })
            ).pipe(
                take(1),
                map(({ payload }) => {
                    if (
                        isKeyringPayload(payload, 'signData') &&
                        payload.return
                    ) {
                        return payload.return;
                    }
                    throw new Error(
                        'Error unknown response for signData message'
                    );
                })
            )
        );
    }

    public setActiveNetworkEnv(network: NetworkEnvType) {
        return lastValueFrom(
            this.sendMessage(
                createMessage<SetNetworkPayload>({
                    type: 'set-network',
                    network,
                })
            ).pipe(take(1))
        );
    }

    public selectAccount(address: SuiAddress) {
        return lastValueFrom(
            this.sendMessage(
                createMessage<KeyringPayload<'switchAccount'>>({
                    type: 'keyring',
                    method: 'switchAccount',
                    args: { address },
                })
            ).pipe(take(1))
        );
    }

    public deriveNextAccount() {
        return lastValueFrom(
            this.sendMessage(
                createMessage<KeyringPayload<'deriveNextAccount'>>({
                    type: 'keyring',
                    method: 'deriveNextAccount',
                })
            ).pipe(
                take(1),
                map(({ payload }) => {
                    if (
                        isKeyringPayload(payload, 'deriveNextAccount') &&
                        payload.return
                    ) {
                        return payload.return.accountAddress;
                    }
                    throw new Error(
                        'Error unknown response for derive account message'
                    );
                })
            )
        );
    }

    importLedgerAccounts(ledgerAccounts: SerializedLedgerAccount[]) {
        return lastValueFrom(
            this.sendMessage(
                createMessage<KeyringPayload<'importLedgerAccounts'>>({
                    type: 'keyring',
                    method: 'importLedgerAccounts',
                    args: { ledgerAccounts },
                })
            ).pipe(take(1))
        );
    }

    public verifyPassword(password: string) {
        return lastValueFrom(
            this.sendMessage(
                createMessage<KeyringPayload<'verifyPassword'>>({
                    type: 'keyring',
                    method: 'verifyPassword',
                    args: { password },
                })
            ).pipe(take(1))
        );
    }

    public exportAccount(password: string, accountAddress: SuiAddress) {
        return lastValueFrom(
            this.sendMessage(
                createMessage<KeyringPayload<'exportAccount'>>({
                    type: 'keyring',
                    method: 'exportAccount',
                    args: { password, accountAddress },
                })
            ).pipe(
                take(1),
                map(({ payload }) => {
                    if (
                        isKeyringPayload(payload, 'exportAccount') &&
                        payload.return
                    ) {
                        return payload.return.keyPair;
                    }
                    throw new Error(
                        'Error unknown response for export account message'
                    );
                })
            )
        );
    }

    public importPrivateKey(password: string, keyPair: ExportedKeypair) {
        return lastValueFrom(
            this.sendMessage(
                createMessage<KeyringPayload<'importPrivateKey'>>({
                    type: 'keyring',
                    method: 'importPrivateKey',
                    args: { password, keyPair },
                })
            ).pipe(take(1))
        );
    }

    private setupAppStatusUpdateInterval() {
        setInterval(() => {
            this.sendAppStatus();
        }, APP_STATUS_UPDATE_INTERVAL);
    }

    private sendAppStatus() {
        const active = document.visibilityState === 'visible';
        this.sendMessage(
            createMessage<KeyringPayload<'appStatusUpdate'>>({
                type: 'keyring',
                method: 'appStatusUpdate',
                args: { active },
            })
        );
    }

    private getWalletStatus() {
        return lastValueFrom(
            this.sendMessage(
                createMessage<KeyringPayload<'walletStatusUpdate'>>({
                    type: 'keyring',
                    method: 'walletStatusUpdate',
                })
            ).pipe(take(1))
        );
    }

    private loadFeatures() {
        return lastValueFrom(
            this.sendMessage(
                createMessage<BasePayload>({
                    type: 'get-features',
                })
            ).pipe(take(1))
        );
    }

    private getNetwork() {
        return lastValueFrom(
            this.sendMessage(
                createMessage<BasePayload>({
                    type: 'get-network',
                })
            ).pipe(take(1))
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
        } else if (
            isKeyringPayload<'walletStatusUpdate'>(
                payload,
                'walletStatusUpdate'
            ) &&
            payload.return
        ) {
            action = setKeyringStatus(payload.return);
        } else if (isLoadedFeaturesPayload(payload)) {
            growthbook.setAttributes(payload.attributes);
            growthbook.setFeatures(payload.features);
        } else if (isSetNetworkPayload(payload)) {
            action = changeActiveNetwork({
                network: payload.network,
            });
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
