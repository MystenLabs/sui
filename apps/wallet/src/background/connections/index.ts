// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import mitt from 'mitt';
import Browser from 'webextension-polyfill';

import { ContentScriptConnection } from './ContentScriptConnection';
import { UiConnection } from './UiConnection';

import type { Connection } from './Connection';
import type { Permission } from '_payloads/permissions';

type ConnectionsEvents = {
    totalUiChanged: number;
    totalCsChanged: number;
};

export class Connections {
    #connections: Connection[] = [];
    #events = mitt<ConnectionsEvents>();
    #totalUiConnections = 0;
    #totalCsConnections = 0;

    constructor() {
        Browser.runtime.onConnect.addListener((port) => {
            try {
                let connection: Connection;
                switch (port.name) {
                    case ContentScriptConnection.CHANNEL:
                        connection = new ContentScriptConnection(port);
                        this.#totalCsConnections++;
                        this.#events.emit(
                            'totalCsChanged',
                            this.#totalCsConnections
                        );
                        break;
                    case UiConnection.CHANNEL:
                        connection = new UiConnection(port);
                        this.#totalUiConnections++;
                        this.#events.emit(
                            'totalUiChanged',
                            this.#totalUiConnections
                        );
                        break;
                    default:
                        throw new Error(
                            `[Connections] Unknown connection ${port.name}`
                        );
                }
                this.#connections.push(connection);
                connection.onDisconnect.subscribe(() => {
                    const connectionIndex =
                        this.#connections.indexOf(connection);
                    if (connectionIndex >= 0) {
                        this.#connections.splice(connectionIndex, 1);
                        if (connection instanceof UiConnection) {
                            this.#totalUiConnections--;
                            this.#events.emit(
                                'totalUiChanged',
                                this.#totalUiConnections
                            );
                        } else if (
                            connection instanceof ContentScriptConnection
                        ) {
                            this.#totalCsConnections--;
                            this.#events.emit(
                                'totalCsChanged',
                                this.#totalCsConnections
                            );
                        }
                    }
                });
            } catch (e) {
                port.disconnect();
            }
        });
    }

    public on = this.#events.on;
    public off = this.#events.off;

    public notifyForPermissionReply(permission: Permission) {
        for (const aConnection of this.#connections) {
            if (
                aConnection instanceof ContentScriptConnection &&
                aConnection.origin === permission.origin
            ) {
                aConnection.permissionReply(permission);
            }
        }
    }

    public notifyForLockedStatusUpdate(isLocked: boolean) {
        for (const aConnection of this.#connections) {
            if (aConnection instanceof UiConnection) {
                aConnection.sendLockedStatusUpdate(isLocked);
            }
        }
    }

    public get totalUiConnections() {
        return this.#totalUiConnections;
    }

    public get totalCsConnections() {
        return this.#totalCsConnections;
    }
}
