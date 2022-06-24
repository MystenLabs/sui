// Copyright (c) 2022, Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import { setPermissions } from '_redux/slices/permissions';

import type { SuiAddress } from '@mysten/sui.js';
import type { AppDispatch } from '_store';

export class BackgroundClient {
    private _dispatch: AppDispatch | null = null;
    private _initialized = false;

    public async init(dispatch: AppDispatch) {
        if (this._initialized) {
            throw new Error('[BackgroundClient] already initialized');
        }
        this._initialized = true;
        this._dispatch = dispatch;
        // TODO: implement
        return this.sendGetPermissionRequests().then(() => undefined);
    }

    public sendPermissionResponse(
        id: string,
        accounts: SuiAddress[],
        allowed: boolean,
        responseDate: string
    ) {
        // TODO: implement
    }

    public async sendGetPermissionRequests() {
        // TODO: remove mock and implement
        const id = /connect\/(.+)/.exec(window.location.hash)?.[1];
        if (this._dispatch && id) {
            this._dispatch(
                setPermissions([
                    {
                        id,
                        accounts: [],
                        allowed: null,
                        createdDate: new Date().toISOString(),
                        favIcon: 'https://www.google.com/favicon.ico',
                        origin: 'https://www.google.com',
                        permissions: ['viewAccount'],
                        responseDate: null,
                    },
                ])
            );
        }
    }
}
