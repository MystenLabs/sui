import { lastValueFrom, take } from 'rxjs';

import { isErrorPayload } from '_messages/payloads/ErrorPayload';
import { PortClient } from '_messaging/PortClient';
import { PortStream } from '_messaging/PortStream';
import { setPermissions } from '_redux/slices/permissions';
import {
    MSG_SENDER_EXTENSION,
    UI_TO_BACKGROUND_CHANNEL_NAME,
} from '_shared/messaging/constants';

import type { SuiAddress } from '@mysten/sui.js';
import type { Message } from '_messages/Message';
import type {
    GetPermissionRequests,
    PermissionRequests,
    PermissionResponse,
} from '_messages/payloads/permissions';
import type { AppDispatch } from '_store';

export class BackgroundClient {
    private _portClient: PortClient;
    private _dispatch: AppDispatch | null = null;
    private _initialized = false;

    constructor() {
        this._portClient = new PortClient(
            UI_TO_BACKGROUND_CHANNEL_NAME,
            MSG_SENDER_EXTENSION
        );
    }

    public async init(dispatch: AppDispatch) {
        if (this._initialized) {
            throw new Error('[BackgroundClient] already initialized');
        }
        this._initialized = true;
        this._dispatch = dispatch;
        this._portClient.onMessage.subscribe((msg) =>
            this.handleIncomingMessage(msg)
        );
        this._portClient.connect();
        return this.sendGetPermissionRequests().then(() => undefined);
    }

    public sendPermissionResponse(
        id: string,
        accounts: SuiAddress[],
        allowed: boolean,
        responseDate: string
    ) {
        this._portClient.sendMessage<PermissionResponse>({
            id,
            type: 'permission-response',
            accounts,
            allowed,
            responseDate,
        });
    }

    public sendGetPermissionRequests() {
        return lastValueFrom(
            this._portClient
                .sendMessage<GetPermissionRequests>({
                    type: 'get-permission-requests',
                })
                .pipe(take(1))
        );
    }

    private handleIncomingMessage(msg: Message) {
        if (!this._initialized || !this._dispatch) {
            throw new Error(
                '[BackgroundClient] is not initialized to handle incoming messages'
            );
        }
        console.log(msg);
        const { payload } = msg;
        if (isErrorPayload(payload)) {
            console.log('[BackgroundClient] Received error message', msg);
            return;
        }

        switch (payload.type) {
            case 'permission-request':
                this._dispatch(
                    setPermissions((payload as PermissionRequests).permissions)
                );
                break;
            default:
                console.log(
                    `[BackgroundClient] payload ${payload.type} is not handled`
                );
        }
    }
}
