// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import { safeParse } from 'valibot';

import { withResolvers } from '../utils/withResolvers.js';
import type { ZkSendRequestType, ZkSendResponsePayload } from './events.js';
import { ZkSendResponse } from './events.js';

const DEFAULT_ZKSEND_ORIGIN = 'https://zksend.com';

export class ZkSendPopup {
	#id: string;
	#origin: string;
	#close?: () => void;

	constructor(origin = DEFAULT_ZKSEND_ORIGIN) {
		this.#id = crypto.randomUUID();
		this.#origin = origin;
	}

	async createRequest<T extends keyof ZkSendRequestType>(
		type: T,
		data?: string,
	): Promise<ZkSendRequestType[T]> {
		const { promise, resolve, reject } = withResolvers<ZkSendRequestType[T]>();

		let popup: Window | null = null;

		const listener = (event: MessageEvent) => {
			if (event.origin !== this.#origin) {
				return;
			}
			const { success, output } = safeParse(ZkSendResponse, event.data);
			if (!success || output.id !== this.#id) return;

			window.removeEventListener('message', listener);

			if (output.payload.type === 'reject') {
				reject(new Error('TODO: Better error message'));
			} else if (output.payload.type === 'resolve') {
				resolve(output.payload.data as ZkSendRequestType[T]);
			}
		};

		this.#close = () => {
			popup?.close();
			window.removeEventListener('message', listener);
		};

		window.addEventListener('message', listener);

		popup = window.open(
			`${origin}/dapp/${type}?${new URLSearchParams({
				id: this.#id,
				origin: this.#origin,
			})}${data ? `#${data}` : ''}`,
		);

		if (!popup) {
			throw new Error('TODO: Better error message');
		}

		return promise;
	}

	close() {
		this.#close?.();
	}
}

export class ZkSendHost {
	#id: string;
	#origin: string;

	constructor(id: string, origin: string) {
		if (typeof window === 'undefined' || !window.opener) {
			throw new Error('TODO: Better error message');
		}

		this.#id = id;
		this.#origin = origin;
	}

	sendMessage(payload: ZkSendResponsePayload) {
		window.opener.postMessage(
			{
				id: this.#id,
				source: 'zksend-channel',
				payload,
			} satisfies ZkSendResponse,
			this.#origin,
		);
	}
}
