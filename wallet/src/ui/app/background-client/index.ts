// Copyright (c) 2022, Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import { lastValueFrom, take } from 'rxjs';

import { createMessage } from '_messages';
import { PortStream } from '_messaging/PortStream';
import { isPermissionRequests } from '_payloads/permissions';
import { setPermissions } from '_redux/slices/permissions';

import type { SuiAddress } from '@mysten/sui.js';
import type { Message } from '_messages';
import type {
    GetPermissionRequests,
    PermissionResponse,
} from '_payloads/permissions';
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
        return this.sendGetPermissionRequests().then(() => undefined);
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
        const responseStream = this.sendMessage(
            createMessage<GetPermissionRequests>({
                type: 'get-permission-requests',
            })
        );
        if (!responseStream) {
            throw new Error('Failed to send get permissions request');
        }
        return lastValueFrom(responseStream.pipe(take(1)));
    }

    private handleIncomingMessage(msg: Message) {
        if (!this._initialized || !this._dispatch) {
            throw new Error(
                'BackgroundClient is not initialized to handle incoming messages'
            );
        }
        const { payload } = msg;
        if (isPermissionRequests(payload)) {
            this._dispatch(setPermissions(payload.permissions));
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
        }
    }
}
