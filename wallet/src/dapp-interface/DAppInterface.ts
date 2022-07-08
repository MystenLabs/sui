// Copyright (c) 2022, Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import { filter, lastValueFrom, map, take } from 'rxjs';

import { createMessage } from '_messages';
import { WindowMessageStream } from '_messaging/WindowMessageStream';
import { isErrorPayload } from '_payloads';
import { ALL_PERMISSION_TYPES } from '_payloads/permissions';

import type { SuiAddress, MoveCallTransaction } from '@mysten/sui.js';
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

function mapToPromise<T extends Payload, R>(
    stream: Observable<T>,
    project: (value: T) => R
) {
    return lastValueFrom(
        stream.pipe(
            take<T>(1),
            map<T, R>((response) => {
                if (isErrorPayload(response)) {
                    // TODO: throw proper error
                    throw new Error(response.message);
                }
                return project(response);
            })
        )
    );
}

export class DAppInterface {
    private _messagesStream: WindowMessageStream;

    constructor() {
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

    public executeMoveCall(transaction: MoveCallTransaction) {
        return mapToPromise(
            this.send<ExecuteTransactionRequest, ExecuteTransactionResponse>({
                type: 'execute-transaction-request',
                transaction,
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
