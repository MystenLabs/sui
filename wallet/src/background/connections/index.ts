import Browser from 'webextension-polyfill';

import { ContentScriptConnection } from './ContentScriptConnection';
import { UiConnection } from './UiConnection';

import type { Connection } from './Connection';

export class Connections {
    private _csConnections: ContentScriptConnection[] = [];
    private _uiConnections: UiConnection[] = [];

    constructor() {
        Browser.runtime.onConnect.addListener((port) => {
            try {
                let connection: Connection;
                switch (port.name) {
                    case ContentScriptConnection.CHANNEL:
                        connection = new ContentScriptConnection(port);
                        connection.onDisconnect.subscribe({
                            next: () => {
                                console.log(
                                    `[Connections] connection disconnected. origin: ${
                                        (connection as ContentScriptConnection)
                                            .origin
                                    }`
                                );
                            },
                        });
                        this._csConnections.push(
                            connection as ContentScriptConnection
                        );
                        break;
                    case UiConnection.CHANNEL:
                        console.log(`New connection from ui`, port);
                        connection = new UiConnection(port);
                        connection.onDisconnect.subscribe({
                            next: () =>
                                console.log(
                                    '[Connections] UiConnection disconnected'
                                ),
                        });
                        this._uiConnections.push(connection as UiConnection);
                        break;
                    default:
                        throw new Error(
                            `[Connections] Unknown connection ${port.name}`
                        );
                }
            } catch (e) {
                port.disconnect();
                console.error(e);
            }
        });
    }
}
