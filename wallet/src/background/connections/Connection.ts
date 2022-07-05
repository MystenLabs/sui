// Copyright (c) 2022, Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import { map, take } from 'rxjs';

import { PortStream } from '_messaging/PortStream';

import type { Message } from '_messages';
import type { Runtime } from 'webextension-polyfill';

export abstract class Connection {
    protected _portStream: PortStream;

    constructor(port: Runtime.Port) {
        this._portStream = new PortStream(port);
        this._portStream.onMessage.subscribe((msg) => this.handleMessage(msg));
    }

    public get onDisconnect() {
        return this._portStream.onDisconnect.pipe(
            map((port) => ({ port, connection: this })),
            take(1)
        );
    }

    protected abstract handleMessage(msg: Message): void;

    protected send(msg: Message) {
        if (this._portStream.connected) {
            return this._portStream.sendMessage(msg);
        }
    }
}
