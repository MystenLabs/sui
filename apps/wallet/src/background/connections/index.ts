// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import { createMessage } from '_messages';
import type { SetNetworkPayload } from '_payloads/network';
import type { Permission } from '_payloads/permissions';
import type { WalletStatusChange, WalletStatusChangePayload } from '_payloads/wallet-status-change';
import type { NetworkEnvType } from '_src/shared/api-env';
import { type UIAccessibleEntityType } from '_src/shared/messaging/messages/payloads/MethodPayload';
import { type QredoConnectPayload } from '_src/shared/messaging/messages/payloads/QredoConnect';
import Browser from 'webextension-polyfill';

import type { Connection } from './Connection';
import { ContentScriptConnection } from './ContentScriptConnection';
import { UiConnection } from './UiConnection';

const appOrigin = new URL(Browser.runtime.getURL('')).origin;

export class Connections {
	#connections: Connection[] = [];

	constructor() {
		Browser.runtime.onConnect.addListener((port) => {
			try {
				let connection: Connection;
				switch (port.name) {
					case ContentScriptConnection.CHANNEL:
						connection = new ContentScriptConnection(port);
						break;
					case UiConnection.CHANNEL:
						if (port.sender?.origin !== appOrigin) {
							throw new Error(
								`[Connections] UI connections are not allowed for origin ${port.sender?.origin}`,
							);
						}
						connection = new UiConnection(port);
						break;
					default:
						throw new Error(`[Connections] Unknown connection ${port.name}`);
				}
				this.#connections.push(connection);
				connection.onDisconnect.subscribe(() => {
					const connectionIndex = this.#connections.indexOf(connection);
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
		messageID?: string,
	) {
		for (const aConnection of this.#connections) {
			if (aConnection instanceof ContentScriptConnection) {
				switch (notification.event) {
					case 'permissionReply':
						aConnection.permissionReply(notification.permission);
						break;
					case 'walletStatusChange':
						if (!('origin' in notification) || aConnection.origin === notification.origin) {
							aConnection.send(
								createMessage<WalletStatusChangePayload>({
									type: 'wallet-status-changed',
									...notification.change,
								}),
							);
						}
						break;
					case 'qredoConnectResult':
						if (notification.origin === aConnection.origin) {
							aConnection.send(
								createMessage<QredoConnectPayload<'connectResponse'>>(
									{
										type: 'qredo-connect',
										method: 'connectResponse',
										args: { allowed: notification.allowed },
									},
									messageID,
								),
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
			| { event: 'storedEntitiesUpdated'; type: UIAccessibleEntityType },
	) {
		for (const aConnection of this.#connections) {
			if (aConnection instanceof UiConnection) {
				switch (notification.event) {
					case 'networkChanged':
						aConnection.send(
							createMessage<SetNetworkPayload>({
								type: 'set-network',
								network: notification.network,
							}),
						);
						break;
					case 'storedEntitiesUpdated':
						aConnection.notifyEntitiesUpdated(notification.type);
						break;
				}
			}
		}
	}
}
