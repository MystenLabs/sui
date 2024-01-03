// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import { parse, safeParse } from 'valibot';

import { withResolvers } from '../utils/withResolvers';
import {
	ZkSendRequest,
	ZkSendRequestType,
	ZkSendResponse,
	ZkSendResponsePayload,
	ZkSendResponsePaylodForType,
} from './events';

const DEFAULT_ZKSEND_ORIGIN = 'https://zksend.com';

export class ZkSendPopup {
	#id: string;
	#origin: string;
	#close?: () => void;

	constructor(origin = DEFAULT_ZKSEND_ORIGIN) {
		// TODO: If we want shorter IDs we can just use nanoID too:
		this.#id = crypto.randomUUID();
		this.#origin = origin;
	}

	async createRequest<T extends ZkSendRequestType>(
		type: T,
		request: Omit<ZkSendRequest, 'id' | 'origin'>,
	): Promise<ZkSendResponsePaylodForType<T>> {
		const { promise, resolve, reject } = withResolvers();

		const params = parse(ZkSendRequest, {
			id: this.#id,
			origin: this.#origin,
			...request,
		});

		const listener = (event: MessageEvent) => {
			if (event.origin !== this.#origin) {
				return;
			}
			const parsed = safeParse(ZkSendResponse, event.data);
			if (!parsed.success) return;

			window.removeEventListener('message', listener);

			if (parsed.output.payload.type === 'reject') {
				reject(new Error('TODO: Better error message'));
			} else {
				resolve(parsed.output.payload);
			}
		};

		let popup: Window | null = null;

		this.#close = () => {
			popup?.close();
			window.removeEventListener('message', listener);
		};

		window.addEventListener('message', listener);

		popup = window.open(`${origin}/dapp/${type}?${new URLSearchParams(params)}`);

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
	request: ZkSendRequest;

	constructor(params: Record<string, unknown>) {
		if (typeof window === 'undefined' || !window.opener) {
			throw new Error('TODO: Better error message');
		}

		this.request = parse(ZkSendRequest, params);
	}

	sendResponse(payload: ZkSendResponsePayload) {
		window.opener.postMessage(
			{
				id: this.request.id,
				source: 'zksend-channel',
				payload,
			} satisfies ZkSendResponse,

			this.request.origin,
		);
	}
}
