import { Connection } from './Connection';
import { isErrorPayload } from '_messages/payloads/ErrorPayload';
import { isPermissionResponse } from '_messages/payloads/permissions';
import { UI_TO_BACKGROUND_CHANNEL_NAME } from '_messaging/constants';
import Permissions from '_src/background/Permissions';

import type {
    Permission,
    PermissionRequests,
} from '_messages/payloads/permissions';
import type { Runtime } from 'webextension-polyfill';

export class UiConnection extends Connection {
    public static readonly CHANNEL = UI_TO_BACKGROUND_CHANNEL_NAME;

    constructor(port: Runtime.Port) {
        console.log(`[UiConnection] New connection from content script`, port);
        super(port);
    }

    protected handleMessages(): void {
        this._portStream.onMessage.subscribe({
            next: async (msg) => {
                console.log(`[UiConnection] received message:`, msg);
                const { payload, id } = msg;
                if (!isErrorPayload(payload)) {
                    if (payload.type === 'get-permission-requests') {
                        this.sendPermissions(
                            await Permissions.getPermissions(),
                            id
                        );
                    } else if (isPermissionResponse(payload)) {
                        Permissions.handlePermissionResponse(payload);
                    }
                }
            },
        });
    }

    private sendPermissions(permissions: Permission[], requestID: string) {
        console.log(
            'sending permissions to ui',
            permissions,
            this._portStream.connected
        );
        if (this._portStream.connected) {
            this._portStream.sendMessage<PermissionRequests>(
                {
                    type: 'permission-request',
                    permissions,
                },
                requestID
            );
        }
    }
}
