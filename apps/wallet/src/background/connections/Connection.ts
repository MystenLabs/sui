// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import type { Message } from '_messages';
import { PortStream } from '_messaging/PortStream';
import { map, take } from 'rxjs';
import type { Runtime } from 'webextension-polyfill';

export abstract class Connection {
	protected _portStream: PortStream;

	constructor(port: Runtime.Port) {
		this._portStream = new PortStream(port);
		this._portStream.onMessage.subscribe((msg) => this.handleMessage(msg));
	}

	public get onDisconnect() {
		return this._portStream.onDisconnect.pipe(
			map((port) => ({ port })),
			take(1),
		);
	}

	public send(msg: Message) {
		if (this._portStream.connected) {
			return this._portStream.sendMessage(msg);
		}
	}

	protected abstract handleMessage(msg: Message): void;
}
