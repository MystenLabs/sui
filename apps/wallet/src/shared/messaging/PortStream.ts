// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import { filter, fromEventPattern, map, share, take, takeUntil, tap } from 'rxjs';
import type { Observable } from 'rxjs';
import Browser from 'webextension-polyfill';
import type { Runtime } from 'webextension-polyfill';

import type { Message } from './messages';
import { isErrorPayload } from './messages/payloads';
import type { PortChannelName } from './PortChannelName';

export class PortStream {
	private _messagesStream: Observable<Message>;
	private _disconnectStream: Observable<Runtime.Port>;
	private _port: Runtime.Port;
	private _connected: boolean;

	public static connectToBackgroundService(name: PortChannelName): PortStream {
		return new PortStream(Browser.runtime.connect({ name }));
	}

	constructor(port: Runtime.Port) {
		this._port = port;
		this._disconnectStream = fromEventPattern<Runtime.Port>(
			(h) => this._port.onDisconnect.addListener(h),
			(h) => this._port.onDisconnect.removeListener(h),
		).pipe(
			take(1),
			tap(() => (this._connected = false)),
			share(),
		);
		this._messagesStream = fromEventPattern<Message>(
			(h) => this._port.onMessage.addListener(h),
			(h) => this._port.onMessage.removeListener(h),
			(msg) => msg,
		).pipe(share(), takeUntil(this._disconnectStream));
		this._connected = true;
	}

	public get onMessage(): Observable<Message> {
		return this._messagesStream;
	}

	public get onDisconnect(): Observable<Runtime.Port> {
		return this._disconnectStream;
	}

	public get connected(): boolean {
		return this._connected;
	}

	public sendMessage(msg: Message): Observable<Message> {
		if (!this._port) {
			throw new Error('Port to background service worker is not defined');
		}
		this._port.postMessage(msg);
		return this.createResponseObservable(msg.id);
	}

	private createResponseObservable(requestMsgID: string): Observable<Message> {
		return this._messagesStream.pipe(
			filter((msg) => msg.id === requestMsgID),
			map((msg) => {
				if (isErrorPayload(msg.payload)) {
					throw new Error(msg.payload.message);
				}
				return msg;
			}),
		);
	}
}
