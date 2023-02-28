// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import { filter, map } from 'rxjs';

import { mapToPromise } from './utils';
import { createMessage } from '_messages';
import { WindowMessageStream } from '_messaging/WindowMessageStream';
import { ALL_PERMISSION_TYPES } from '_payloads/permissions';

import type {
    SuiAddress,
    MoveCallTransaction,
    SignableTransaction,
} from '@mysten/sui.js';
import type { Payload } from '_payloads';
import type { GetAccount } from '_payloads/account/GetAccount';
import type { GetAccountResponse } from '_payloads/account/GetAccountResponse';
import type {
    PermissionType,
    HasPermissionsRequest,
    HasPermissionsResponse,
    AcquirePermissionsRequest,
    AcquirePermissionsResponse,
} from '_payloads/permissions';
import type {
    ExecuteTransactionRequest,
    ExecuteTransactionResponse,
} from '_payloads/transactions';
import type { Observable } from 'rxjs';

export class DAppInterface {
    private _messagesStream: WindowMessageStream;

    constructor() {
        // eslint-disable-next-line no-console
        console.warn(
            'Your application is using the global `suiWindow` interface, which is not recommended for applications. Please migrate to Wallet Adapters: https://github.com/MystenLabs/sui/tree/main/sdk/wallet-adapter'
        );

        this._messagesStream = new WindowMessageStream(
            'sui_in-page',
            'sui_content-script'
        );
    }

    public hasPermissions(
        permissions: readonly PermissionType[] = ALL_PERMISSION_TYPES
    ): Promise<boolean> {
        return mapToPromise(
            this.send<HasPermissionsRequest, HasPermissionsResponse>({
                type: 'has-permissions-request',
                permissions,
            }),
            (response) => response.result
        );
    }

    public requestPermissions(
        permissions: readonly PermissionType[] = ALL_PERMISSION_TYPES
    ): Promise<boolean> {
        return mapToPromise(
            this.send<AcquirePermissionsRequest, AcquirePermissionsResponse>({
                type: 'acquire-permissions-request',
                permissions,
            }),
            (response) => response.result
        );
    }

    public getAccounts(): Promise<SuiAddress[]> {
        return mapToPromise(
            this.send<GetAccount, GetAccountResponse>({
                type: 'get-account',
            }),
            (response) => response.accounts
        );
    }

    public async signAndExecuteTransaction(transaction: SignableTransaction) {
        return mapToPromise(
            this.send<ExecuteTransactionRequest, ExecuteTransactionResponse>({
                type: 'execute-transaction-request',
                transaction: {
                    type: 'v2',
                    data: transaction,
                    account: (await this.getAccounts())[0],
                },
            }),
            (response) => response.result
        );
    }

    public async executeMoveCall(transaction: MoveCallTransaction) {
        // eslint-disable-next-line no-console
        console.warn(
            'You are using the deprecated `executeMoveCall` method on the `suiWallet` interface. This method will be removed in a future release of the Sui Wallet. Please migrate to the new `signAndExecuteTransaction` method.'
        );

        return mapToPromise(
            this.send<ExecuteTransactionRequest, ExecuteTransactionResponse>({
                type: 'execute-transaction-request',
                transaction: {
                    type: 'move-call',
                    data: transaction,
                    account: (await this.getAccounts())[0],
                },
            }),
            (response) => response.result
        );
    }

    public async executeSerializedMoveCall(tx: string | Uint8Array) {
        // eslint-disable-next-line no-console
        console.warn(
            'You are using the deprecated `executeSerializedMoveCall` method on the `suiWallet` interface. This method will be removed in a future release of the Sui Wallet. Please migrate to the new `signAndExecuteTransaction` method.'
        );

        const data =
            typeof tx === 'string' ? tx : Buffer.from(tx).toString('base64');
        return mapToPromise(
            this.send<ExecuteTransactionRequest, ExecuteTransactionResponse>({
                type: 'execute-transaction-request',
                transaction: {
                    type: 'serialized-move-call',
                    data,
                    account: (await this.getAccounts())[0],
                },
            }),
            (response) => response.result
        );
    }

    private send<
        RequestPayload extends Payload,
        ResponsePayload extends Payload | void = void
    >(
        payload: RequestPayload,
        responseForID?: string
    ): Observable<ResponsePayload> {
        const msg = createMessage(payload, responseForID);
        this._messagesStream.send(msg);
        return this._messagesStream.messages.pipe(
            filter(({ id }) => id === msg.id),
            map((msg) => msg.payload as ResponsePayload)
        );
    }
}
