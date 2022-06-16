import Browser from 'webextension-polyfill';

import {
    CONTENT_TO_BACKGROUND_CHANNEL_NAME,
    MSG_SENDER_CONTENT,
    MSG_SENDER_DAPP,
    MSG_SENDER_FIELD,
} from '_shared/messaging/constants';

import type { Runtime } from 'webextension-polyfill';

class LazyBGScriptConnection {
    private connected = false;
    private bgPort: Runtime.Port | null = null;

    public send<M>(msg: M) {
        if (!this.connected) {
            this.createConnection();
        }
        this.bgPort?.postMessage(msg);
    }

    private createConnection() {
        if (this.connected) {
            return;
        }
        this.bgPort = Browser.runtime.connect({
            name: CONTENT_TO_BACKGROUND_CHANNEL_NAME,
        });
        this.connected = true;
        this.bgPort.onMessage.addListener((msg) => {
            //forward to dapp interface
            window.postMessage({
                ...msg,
                [MSG_SENDER_FIELD]: MSG_SENDER_CONTENT,
            });
        });
        this.bgPort.onDisconnect.addListener((port) => {
            console.log('Content script port to bg disconnected', port);
            this.connected = false;
            this.bgPort = null;
        });
    }
}

export function setupMessagesProxy() {
    const proxy = new LazyBGScriptConnection();

    window.addEventListener('message', (event) => {
        if (
            event.source === window &&
            event.data[MSG_SENDER_FIELD] === MSG_SENDER_DAPP
        ) {
            // forward to bg
            proxy.send(event.data);
        }
    });
}
