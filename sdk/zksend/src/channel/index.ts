// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import { atom, WritableAtom } from 'nanostores';
import { parse, safeParse } from 'valibot';

import { withResolvers } from '../utils/withResolvers';
import {
	ZkSendRequest,
	ZkSendRequestType,
	ZkSendResolveResponse,
	ZkSendResponse,
	ZkSendResponsePayload,
} from './events';

const DEFAULT_ZKSEND_ORIGIN = 'https://zksend.com';

export class ZkSendPopup {
	#id: string;
	#origin: string;
	#close?: () => void;

	constructor(origin = DEFAULT_ZKSEND_ORIGIN) {
		this.#id = crypto.randomUUID();
		this.#origin = origin;
	}

	async createRequest(
		type: ZkSendRequestType,
		partialRequest: Omit<ZkSendRequest, 'id' | 'origin'>,
	): Promise<ZkSendResolveResponse['data']> {
		const { promise, resolve, reject } = withResolvers<ZkSendResolveResponse['data']>();

		const request = parse(ZkSendRequest, {
			id: this.#id,
			...partialRequest,
		} satisfies ZkSendRequest);

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
				resolve(output.payload.data);
			} else if (output.payload.type === 'ready') {
				if (!popup) {
					throw new Error('TODO: Better error message');
				}

				popup.postMessage(request, this.#origin);
			}
		};

		this.#close = () => {
			popup?.close();
			window.removeEventListener('message', listener);
		};

		window.addEventListener('message', listener);

		popup = window.open(
			`${origin}/dapp/${type}?${new URLSearchParams({ id: this.#id, origin: this.#origin })}`,
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

	$request: WritableAtom<ZkSendRequest | null>;

	constructor(id: string, origin: string) {
		if (typeof window === 'undefined' || !window.opener) {
			throw new Error('TODO: Better error message');
		}

		this.#id = id;
		this.#origin = origin;
		this.$request = atom(null);

		window.addEventListener('message', this.#listener);

		this.sendMessage({ type: 'ready' });
	}

	#listener = (event: MessageEvent) => {
		if (event.origin !== this.#origin) return;
		const { success, output } = safeParse(ZkSendRequest, event.data);
		if (!success || output.id !== this.#id) return;

		this.$request.set(output);
	};

	close() {
		window.removeEventListener('message', this.#listener);
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
