// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import { map, Subject, take } from 'rxjs';

import { PortStream } from '_messaging/PortStream';

import type { Message } from '_messages';
import type { Runtime } from 'webextension-polyfill';

export abstract class Connection {
    protected _portStream: PortStream;

    constructor(port: Runtime.Port) {
        this._portStream = new PortStream(port);
        this._portStream.onMessage.subscribe((msg) => this._handleMessage(msg));
    }

    public disconnect() {
        this._portStream.disconnect();
    }

    public get onDisconnect() {
        return this._portStream.onDisconnect.pipe(
            map((port) => ({ port, connection: this })),
            take(1)
        );
    }

    public send(msg: Message) {
        if (this._portStream.connected) {
            return this._portStream.sendMessage(msg);
        }
    }

    public onMessage = new Subject<Message>();

    private _handleMessage(msg: Message): void {
        this.onMessage.next(msg);
        this.handleMessage(msg);
    }

    protected abstract handleMessage(msg: Message): void;
}
