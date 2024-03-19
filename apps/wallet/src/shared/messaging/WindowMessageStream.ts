// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import type { Message } from '_messages';
import { filter, fromEvent, map, share } from 'rxjs';
import type { Observable } from 'rxjs';

export type ClientType = 'sui_in-page' | 'sui_content-script';

type WindowMessage = {
	target: ClientType;
	payload: Message;
};

export class WindowMessageStream {
	public readonly messages: Observable<Message>;
	private _name: ClientType;
	private _target: ClientType;

	constructor(name: ClientType, target: ClientType) {
		if (name === target) {
			throw new Error('[WindowMessageStream] name and target must be different');
		}
		this._name = name;
		this._target = target;
		this.messages = fromEvent<MessageEvent<WindowMessage>>(window, 'message').pipe(
			filter((message) => message.source === window && message.data.target === this._name),
			map((message) => message.data.payload),
			share(),
		);
	}

	public send(payload: Message) {
		const msg: WindowMessage = {
			target: this._target,
			payload,
		};
		window.postMessage(msg);
	}
}
