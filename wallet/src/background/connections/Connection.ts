import { map } from 'rxjs';

import { PortStream } from '_messaging/PortStream';
import { MSG_SENDER_BACKGROUND } from '_messaging/constants';

import type { Runtime } from 'webextension-polyfill';

export abstract class Connection {
    protected _portStream: PortStream;

    constructor(port: Runtime.Port) {
        this._portStream = new PortStream(port, MSG_SENDER_BACKGROUND);
        this.handleMessages();
    }

    public get onDisconnect() {
        return this._portStream.onDisconnect.pipe(
            map((port) => ({ port, connection: this }))
        );
    }

    protected abstract handleMessages(): void;
}
