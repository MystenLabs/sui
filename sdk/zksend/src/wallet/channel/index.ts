// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import type { InferOutput } from 'valibot';
import { parse, safeParse } from 'valibot';

import { withResolvers } from '../../utils/withResolvers.js';
import type { StashedRequestData, StashedResponsePayload, StashedResponseTypes } from './events.js';
import { StashedRequest, StashedResponse } from './events.js';

export const DEFAULT_STASHED_ORIGIN = 'https://getstashed.com';

export { StashedRequest, StashedResponse };

export class StashedPopup {
	#popup: Window;

	#id: string;
	#origin: string;
	#name: string;

	#promise: Promise<unknown>;
	#resolve: (data: unknown) => void;
	#reject: (error: Error) => void;

	#interval: ReturnType<typeof setInterval> | null = null;

	constructor({ name, origin = DEFAULT_STASHED_ORIGIN }: { name: string; origin?: string }) {
		const popup = window.open('about:blank', '_blank');

		if (!popup) {
			throw new Error('Failed to open new window');
		}
		this.#popup = popup;

		this.#id = crypto.randomUUID();
		this.#origin = origin;
		this.#name = name;

		const { promise, resolve, reject } = withResolvers();
		this.#promise = promise;
		this.#resolve = resolve;
		this.#reject = reject;

		this.#interval = setInterval(() => {
			try {
				if (this.#popup.closed) {
					this.#cleanup();
					reject(new Error('User closed the Stashed window'));
				}
			} catch {
				// This can error during the login flow, but that's fine.
			}
		}, 1000);
	}

	send<T extends StashedRequestData['type']>({
		type,
		...data
	}: {
		type: T;
	} & Extract<StashedRequestData, { type: T }>): Promise<StashedResponseTypes[T]> {
		window.addEventListener('message', this.#listener);
		this.#popup.location.assign(
			`${this.#origin}/dapp/${type}?${new URLSearchParams({
				id: this.#id,
				origin: window.origin,
				name: this.#name,
			})}${data ? `#${new URLSearchParams(data as never)}` : ''}`,
		);

		return this.#promise as Promise<StashedResponseTypes[T]>;
	}

	close() {
		this.#cleanup();
		this.#popup.close();
	}

	#listener = (event: MessageEvent) => {
		if (event.origin !== this.#origin) {
			return;
		}
		const { success, output } = safeParse(StashedResponse, event.data);
		if (!success || output.id !== this.#id) return;

		this.#cleanup();

		if (output.payload.type === 'reject') {
			this.#reject(new Error('User rejected the request'));
		} else if (output.payload.type === 'resolve') {
			this.#resolve(output.payload.data);
		}
	};

	#cleanup() {
		if (this.#interval) {
			clearInterval(this.#interval);
			this.#interval = null;
		}
		window.removeEventListener('message', this.#listener);
	}
}

export class StashedHost {
	#request: InferOutput<typeof StashedRequest>;

	constructor(request: InferOutput<typeof StashedRequest>) {
		if (typeof window === 'undefined' || !window.opener) {
			throw new Error(
				'StashedHost can only be used in a window opened through `window.open`. `window.opener` is not available.',
			);
		}

		this.#request = request;
	}

	static fromUrl(url: string = window.location.href) {
		const parsed = new URL(url);

		const urlHashData = parsed.hash
			? Object.fromEntries(
					[...new URLSearchParams(parsed.hash.slice(1))].map(([key, value]) => [
						key,
						value.replace(/ /g, '+'),
					]),
				)
			: {};

		const request = parse(StashedRequest, {
			id: parsed.searchParams.get('id'),
			origin: parsed.searchParams.get('origin'),
			name: parsed.searchParams.get('name'),
			payload: {
				type: parsed.pathname.split('/').pop(),
				...urlHashData,
			},
		});

		return new StashedHost(request);
	}

	getRequestData() {
		return this.#request;
	}

	sendMessage(payload: StashedResponsePayload) {
		window.opener.postMessage(
			{
				id: this.#request.id,
				source: 'zksend-channel',
				payload,
			} satisfies StashedResponse,
			this.#request.origin,
		);
	}

	close(payload?: StashedResponsePayload) {
		if (payload) {
			this.sendMessage(payload);
		}
		window.close();
	}
}
