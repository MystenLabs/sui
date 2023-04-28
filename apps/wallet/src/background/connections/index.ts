// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import Browser from 'webextension-polyfill';

import { ContentScriptConnection } from './ContentScriptConnection';
import { KeepAliveConnection } from './KeepAliveConnection';
import { UiConnection } from './UiConnection';
import { createMessage } from '_messages';
import { KEEP_ALIVE_BG_PORT_NAME } from '_src/content-script/keep-bg-alive';
import { type QredoConnectPayload } from '_src/shared/messaging/messages/payloads/QredoConnect';

import type { NetworkEnvType } from '../NetworkEnv';
import type { Connection } from './Connection';
import type { SetNetworkPayload } from '_payloads/network';
import type { Permission } from '_payloads/permissions';
import type {
    WalletStatusChange,
    WalletStatusChangePayload,
} from '_payloads/wallet-status-change';

export class Connections {
    #connections: (Connection | KeepAliveConnection)[] = [];

    constructor() {
        Browser.runtime.onConnect.addListener((port) => {
            try {
                let connection: Connection | KeepAliveConnection;
                switch (port.name) {
                    case ContentScriptConnection.CHANNEL:
                        connection = new ContentScriptConnection(port);
                        break;
                    case UiConnection.CHANNEL:
                        connection = new UiConnection(port);
                        break;
                    case KEEP_ALIVE_BG_PORT_NAME:
                        connection = new KeepAliveConnection(port);
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
                    }
                });
            } catch (e) {
                port.disconnect();
            }
        });
    }

    public notifyContentScript(
        notification:
            | { event: 'permissionReply'; permission: Permission }
            | {
                  event: 'walletStatusChange';
                  change: Omit<WalletStatusChange, 'accounts'>;
              }
            | {
                  event: 'walletStatusChange';
                  origin: string;
                  change: WalletStatusChange;
              }
            | {
                  event: 'qredoConnectResult';
                  origin: string;
                  allowed: boolean;
              },
        messageID?: string
    ) {
        for (const aConnection of this.#connections) {
            if (aConnection instanceof ContentScriptConnection) {
                switch (notification.event) {
                    case 'permissionReply':
                        aConnection.permissionReply(notification.permission);
                        break;
                    case 'walletStatusChange':
                        if (
                            !('origin' in notification) ||
                            aConnection.origin === notification.origin
                        ) {
                            aConnection.send(
                                createMessage<WalletStatusChangePayload>({
                                    type: 'wallet-status-changed',
                                    ...notification.change,
                                })
                            );
                        }
                        break;
                    case 'qredoConnectResult':
                        if (notification.origin === aConnection.origin) {
                            aConnection.send(
                                createMessage<
                                    QredoConnectPayload<'connectResponse'>
                                >(
                                    {
                                        type: 'qredo-connect',
                                        method: 'connectResponse',
                                        args: { allowed: notification.allowed },
                                    },
                                    messageID
                                )
                            );
                        }
                        break;
                }
            }
        }
    }

    public notifyUI(
        notification:
            | { event: 'networkChanged'; network: NetworkEnvType }
            | { event: 'lockStatusUpdate'; isLocked: boolean }
    ) {
        for (const aConnection of this.#connections) {
            if (aConnection instanceof UiConnection) {
                switch (notification.event) {
                    case 'networkChanged':
                        aConnection.send(
                            createMessage<SetNetworkPayload>({
                                type: 'set-network',
                                network: notification.network,
                            })
                        );
                        break;
                    case 'lockStatusUpdate':
                        aConnection.sendLockedStatusUpdate(
                            notification.isLocked
                        );
                        break;
                }
            }
        }
    }
}
