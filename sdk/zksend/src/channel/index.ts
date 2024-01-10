// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import type { Output } from 'valibot';
import { safeParse } from 'valibot';

import { withResolvers } from '../utils/withResolvers.js';
import type { ZkSendRequestTypes, ZkSendResponsePayload, ZkSendResponseTypes } from './events.js';
import { ZkSendRequest, ZkSendResponse } from './events.js';

export const DEFAULT_ZKSEND_ORIGIN = 'https://zksend.com';

interface ZkSendPopupOptions {
	origin?: string;
	name: string;
}

export class ZkSendPopup {
	#id: string;
	#origin: string;
	#name: string;

	#close?: () => void;

	constructor({ origin = DEFAULT_ZKSEND_ORIGIN, name }: ZkSendPopupOptions) {
		this.#id = crypto.randomUUID();
		this.#origin = origin;
		this.#name = name;
	}

	async createRequest<T extends keyof ZkSendResponseTypes>(
		type: T,
		data: ZkSendRequestTypes[T],
	): Promise<ZkSendResponseTypes[T]> {
		const { promise, resolve, reject } = withResolvers<ZkSendResponseTypes[T]>();

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
				resolve(output.payload.data as ZkSendResponseTypes[T]);
			}
		};

		this.#close = () => {
			popup?.close();
			window.removeEventListener('message', listener);
		};

		window.addEventListener('message', listener);

		popup = window.open(
			`${this.#origin}/dapp/${type}?${new URLSearchParams({
				id: this.#id,
				origin: window.origin,
				name: this.#name,
			})}${data ? `#${new URLSearchParams(data)}` : ''}`,
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
	#request: Output<typeof ZkSendRequest>;

	constructor(request: Output<typeof ZkSendRequest>) {
		if (typeof window === 'undefined' || !window.opener) {
			throw new Error('TODO: Better error message');
		}

		this.#request = request;
	}

	static fromUrl(url: string = window.location.href) {
		const parsed = new URL(url);

		const request = safeParse(ZkSendRequest, {
			id: parsed.searchParams.get('id'),
			origin: parsed.searchParams.get('origin'),
			name: parsed.searchParams.get('name'),
			type: parsed.pathname.split('/').pop(),
			data: parsed.hash ? Object.fromEntries(new URLSearchParams(parsed.hash.slice(1))) : {},
		});

		if (request.issues) {
			throw new Error('TODO: Better error message');
		}

		return new ZkSendHost(request.output);
	}

	getRequestData() {
		return this.#request;
	}

	sendMessage(payload: ZkSendResponsePayload) {
		window.opener.postMessage(
			{
				id: this.#request.id,
				source: 'zksend-channel',
				payload,
			} satisfies ZkSendResponse,
			this.#request.origin,
		);
	}
}
