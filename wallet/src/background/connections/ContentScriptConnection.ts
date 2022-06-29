// Copyright (c) 2022, Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import { Connection } from './Connection';
import { createMessage } from '_messages';
import { isGetAccount } from '_payloads/account/GetAccount';
import Permissions from '_src/background/Permissions';

import type { SuiAddress } from '@mysten/sui.js';
import type { Message } from '_messages';
import type { PortChannelName } from '_messaging/PortChannelName';
import type { ErrorPayload } from '_payloads';
import type { GetAccountResponse } from '_payloads/account/GetAccountResponse';
import type { Runtime } from 'webextension-polyfill';

export class ContentScriptConnection extends Connection {
    public static readonly CHANNEL: PortChannelName =
        'sui_content<->background';
    public readonly origin: string;
    public readonly originFavIcon: string | undefined;

    constructor(port: Runtime.Port) {
        super(port);
        this.origin = this.getOrigin(port);
        this.originFavIcon = port.sender?.tab?.favIconUrl;
    }

    protected async handleMessage(msg: Message) {
        const { payload } = msg;
        if (isGetAccount(payload)) {
            try {
                const permission = await Permissions.acquirePermissions(
                    ['viewAccount'],
                    this
                );
                this.sendAccounts(permission.accounts, msg.id);
            } catch (e) {
                this.sendError(
                    {
                        error: true,
                        message: (e as Error).toString(),
                        code: -1,
                    },
                    msg.id
                );
            }
        }
    }

    private getOrigin(port: Runtime.Port) {
        if (port.sender?.origin) {
            return port.sender.origin;
        }
        if (port.sender?.url) {
            return new URL(port.sender.url).origin;
        }
        throw new Error(
            "[ContentScriptConnection] port doesn't include an origin"
        );
    }

    private sendError<Error extends ErrorPayload>(
        error: Error,
        responseForID?: string
    ) {
        this.send(createMessage(error, responseForID));
    }

    private sendAccounts(accounts: SuiAddress[], responseForID?: string) {
        this.send(
            createMessage<GetAccountResponse>(
                {
                    type: 'get-account-response',
                    accounts,
                },
                responseForID
            )
        );
    }
}
