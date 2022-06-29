// Copyright (c) 2022, Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import Browser from 'webextension-polyfill';

import { ContentScriptConnection } from './ContentScriptConnection';
import { UiConnection } from './UiConnection';

import type { Connection } from './Connection';

export class Connections {
    private _connections: Connection[] = [];

    constructor() {
        Browser.runtime.onConnect.addListener((port) => {
            try {
                let connection: Connection;
                switch (port.name) {
                    case ContentScriptConnection.CHANNEL:
                        connection = new ContentScriptConnection(port);
                        break;
                    case UiConnection.CHANNEL:
                        connection = new UiConnection(port);
                        break;
                    default:
                        throw new Error(
                            `[Connections] Unknown connection ${port.name}`
                        );
                }
                this._connections.push(connection);
                connection.onDisconnect.subscribe(() => {
                    const connectionIndex =
                        this._connections.indexOf(connection);
                    if (connectionIndex >= 0) {
                        this._connections.splice(connectionIndex, 1);
                    }
                });
            } catch (e) {
                port.disconnect();
            }
        });
    }
}
