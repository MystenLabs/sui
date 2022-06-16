import { Connection } from './Connection';
import { isErrorPayload } from '_messages/payloads/ErrorPayload';
import { CONTENT_TO_BACKGROUND_CHANNEL_NAME } from '_messaging/constants';
import Permissions from '_src/background/Permissions';

import type { SuiAddress } from '@mysten/sui.js';
import type { ErrorPayload } from '_messages/payloads/ErrorPayload';
import type { GetAccountResponse } from '_src/shared/messaging/messages/payloads/account/GetAccountResponse';
import type { Runtime } from 'webextension-polyfill';

export class ContentScriptConnection extends Connection {
    public static CHANNEL = CONTENT_TO_BACKGROUND_CHANNEL_NAME;
    public readonly origin: string;

    constructor(port: Runtime.Port) {
        console.log(
            `[ContentScriptConnection] New connection from content script`,
            port
        );
        super(port);
        this.origin = this.getOrigin(port);
    }

    protected handleMessages() {
        this._portStream.onMessage.subscribe({
            complete: () =>
                console.log('[ContentScriptConnection] stream completed'),
            next: async (msg) => {
                console.log(
                    '[ContentScriptConnection] new incoming message',
                    msg
                );
                const { payload } = msg;
                if (!isErrorPayload(payload)) {
                    if (payload.type === 'get-account') {
                        try {
                            const permission =
                                await Permissions.acquirePermission(
                                    'viewAccount',
                                    this
                                );
                            this.sendAccounts(permission.accounts, msg.id);
                        } catch (e) {
                            this.sendError(
                                {
                                    error: true,
                                    message: (e as Error).toString(),
                                    code: -1,
                                    data: null,
                                },
                                msg.id
                            );
                            console.log('Acquiring permission failed', e);
                        }
                    }
                } else {
                    // TODO: handle error payload
                }
            },
        });
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

    private sendError<T = void>(e: ErrorPayload<T>, responseForID?: string) {
        if (this._portStream.connected) {
            this._portStream.sendMessage(e, responseForID);
        }
    }

    private sendAccounts(accounts: SuiAddress[], responseForID?: string) {
        if (this._portStream.connected) {
            this._portStream.sendMessage<GetAccountResponse>(
                {
                    type: 'get-account-response',
                    accounts,
                },
                responseForID
            );
        }
    }
}
