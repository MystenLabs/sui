// Copyright (c) 2022, Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import Messages from '../Messages';
import { Connection } from './Connection';
import { createMessage } from '_messages';
import { isGetSignMessageRequests } from '_payloads/messages/ui/GetSignMessageRequests';
import { isSignMessageRequestResponse } from '_payloads/messages/ui/SignMessageRequestResponse';
import {
    isGetPermissionRequests,
    isPermissionResponse,
} from '_payloads/permissions';
import { isGetTransactionRequests } from '_payloads/transactions/ui/GetTransactionRequests';
import { isTransactionRequestResponse } from '_payloads/transactions/ui/TransactionRequestResponse';
import Permissions from '_src/background/Permissions';
import Transactions from '_src/background/Transactions';

import type { Message } from '_messages';
import type { PortChannelName } from '_messaging/PortChannelName';
import type { SignMessageRequest } from '_payloads/messages/SignMessageRequest';
import type { GetSignMessageRequestsResponse } from '_payloads/messages/ui/GetSignMessageRequestsResponse';
import type { Permission, PermissionRequests } from '_payloads/permissions';
import type { TransactionRequest } from '_payloads/transactions';
import type { GetTransactionRequestsResponse } from '_payloads/transactions/ui/GetTransactionRequestsResponse';

export class UiConnection extends Connection {
    public static readonly CHANNEL: PortChannelName = 'sui_ui<->background';

    protected async handleMessage(msg: Message) {
        const { payload, id } = msg;
        if (isGetPermissionRequests(payload)) {
            this.sendPermissions(
                Object.values(await Permissions.getPermissions()),
                id
            );
        } else if (isPermissionResponse(payload)) {
            Permissions.handlePermissionResponse(payload);
        } else if (isTransactionRequestResponse(payload)) {
            Transactions.handleMessage(payload);
        } else if (isSignMessageRequestResponse(payload)) {
            Messages.handleMessage(payload);
        } else if (isGetTransactionRequests(payload)) {
            this.sendTransactionRequests(
                Object.values(await Transactions.getTransactionRequests()),
                id
            );
        } else if (isGetSignMessageRequests(payload)) {
            this.sendSignMessageRequests(
                Object.values(await Messages.getSignMessageRequests()),
                id
            );
        }
    }

    private sendPermissions(permissions: Permission[], requestID: string) {
        this.send(
            createMessage<PermissionRequests>(
                {
                    type: 'permission-request',
                    permissions,
                },
                requestID
            )
        );
    }

    private sendTransactionRequests(
        txRequests: TransactionRequest[],
        requestID: string
    ) {
        this.send(
            createMessage<GetTransactionRequestsResponse>(
                {
                    type: 'get-transaction-requests-response',
                    txRequests,
                },
                requestID
            )
        );
    }

    private sendSignMessageRequests(
        signMessageRequests: SignMessageRequest[],
        requestID: string
    ) {
        this.send(
            createMessage<GetSignMessageRequestsResponse>(
                {
                    type: 'get-sign-message-requests-response',
                    signMessageRequests,
                },
                requestID
            )
        );
    }
}
