// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import EventEmitter from 'node:events';
import { beforeEach, describe, expect, it, vi } from 'vitest';

import { SuiHTTPTransport } from '../../../src/client';
import { PACKAGE_VERSION, TARGETED_RPC_VERSION } from '../../../src/version';

describe('SuiHTTPTransport', () => {
	describe('rpc requests', () => {
		const mockResult = { data: 123 };
		let requestId = 0;

		const fetch = vi.fn(() => {
			requestId += 1;
			return Promise.resolve(
				new Response(
					new TextEncoder().encode(
						JSON.stringify({
							jsonrpc: '2.0',
							result: mockResult,
							id: requestId,
						}),
					),
					{
						status: 200,
					},
				),
			);
		});

		const transport = new SuiHTTPTransport({
			url: 'http://localhost:4000',
			rpc: {
				url: 'http://localhost:4000',
			},
			fetch,
		});

		beforeEach(() => {
			fetch.mockClear();
		});

		it('should make a request', async () => {
			const result = await transport.request({
				method: 'getAllBalances',
				params: ['0x1234'],
			});

			expect(fetch).toHaveBeenCalledTimes(1);

			expect(fetch).toHaveBeenCalledWith('http://localhost:4000', {
				body: JSON.stringify({
					jsonrpc: '2.0',
					id: requestId,
					method: 'getAllBalances',
					params: ['0x1234'],
				}),
				headers: {
					'Content-Type': 'application/json',
					'Client-Sdk-Type': 'typescript',
					'Client-Sdk-Version': PACKAGE_VERSION,
					'Client-Target-Api-Version': TARGETED_RPC_VERSION,
				},
				method: 'POST',
			});

			expect(result).toEqual(mockResult);
		});
	});

	describe('rpc subscriptions', () => {
		let sockets: (WebSocket & EventEmitter)[] = [];
		let sentMessages: unknown[] = [];
		let subscriptionId = 100;
		const results = new Map<string, object>();
		const MockWebSocketConstructor = vi.fn(() => {
			const socket = new EventEmitter() as unknown as WebSocket & EventEmitter;
			socket.addEventListener = vi.fn(socket.addListener.bind(socket));
			socket.close = vi.fn();
			socket.send = vi.fn((message: string) => {
				const data = JSON.parse(message);
				sentMessages.push(data);

				if (data.id && data.method) {
					setTimeout(() => {
						socket.emit('message', {
							data: JSON.stringify({
								jsonrpc: '2.0',
								id: data.id,
								result: data.method.startsWith('subscribe') ? subscriptionId++ : {},
								...results.get(data.method),
							}),
						});
					});
				}
			});
			sockets.push(socket);

			setTimeout(() => {
				socket.emit('open');
			}, 10);

			return socket;
		}) as unknown as typeof WebSocket;

		beforeEach(() => {
			subscriptionId = 100;
			sockets = [];
			sentMessages = [];
		});

		it('Creates a subscription', async () => {
			const transport = new SuiHTTPTransport({
				url: 'http://localhost:4000',
				WebSocketConstructor: MockWebSocketConstructor,
			});
			const onMessage = vi.fn();
			const unsubscribe = await transport.subscribe({
				method: 'subscribeExample',
				unsubscribe: 'unsubscribeExample',
				params: [],
				onMessage,
			});

			expect(sockets.length).toEqual(1);
			const socket = sockets[0];

			expect(socket.addEventListener).toHaveBeenCalledTimes(3);
			expect(socket.addEventListener).toHaveBeenCalledWith('open', expect.any(Function));
			expect(socket.addEventListener).toHaveBeenCalledWith('message', expect.any(Function));
			expect(socket.addEventListener).toHaveBeenCalledWith('close', expect.any(Function));
			expect(sentMessages).toEqual([
				{
					jsonrpc: '2.0',
					id: 1,
					method: 'subscribeExample',
					params: [],
				},
			]);

			expect(onMessage).toHaveBeenCalledTimes(0);

			const mockEvent = {
				id: 123,
			};

			socket.emit('message', {
				data: JSON.stringify({
					jsonrpc: '2.0',
					params: {
						subscription: subscriptionId - 1,
						result: mockEvent,
					},
				}),
			});

			expect(onMessage).toHaveBeenCalledTimes(1);
			expect(onMessage).toHaveBeenCalledWith(mockEvent);

			await new Promise((resolve) => setTimeout(resolve, 10));

			await unsubscribe();

			expect(sentMessages).toEqual([
				{
					jsonrpc: '2.0',
					id: 1,
					method: 'subscribeExample',
					params: [],
				},
				{
					jsonrpc: '2.0',
					id: 2,
					method: 'unsubscribeExample',
					params: [subscriptionId - 1],
				},
			]);
		});

		it('Should reconnect on close', async () => {
			const transport = new SuiHTTPTransport({
				url: 'http://localhost:4000',
				WebSocketConstructor: MockWebSocketConstructor,
				websocket: {
					reconnectTimeout: 1,
				},
			});
			const onMessage = vi.fn();
			const unsubscribe = await transport.subscribe({
				method: 'subscribeExample',
				unsubscribe: 'unsubscribeExample',
				params: [],
				onMessage,
			});

			expect(sockets.length).toEqual(1);
			const socket1 = sockets[0];

			expect(sentMessages).toEqual([
				{
					jsonrpc: '2.0',
					id: 1,
					method: 'subscribeExample',
					params: [],
				},
			]);

			expect(onMessage).toHaveBeenCalledTimes(0);
			socket1.emit('close');

			await new Promise((resolve) => setTimeout(resolve, 100));

			expect(socket1.close).toHaveBeenCalled();
			expect(sockets.length).toEqual(2);

			const socket2 = sockets[1];

			expect(socket2.addEventListener).toHaveBeenCalledTimes(3);
			expect(socket2.addEventListener).toHaveBeenCalledWith('open', expect.any(Function));
			expect(socket2.addEventListener).toHaveBeenCalledWith('message', expect.any(Function));
			expect(socket2.addEventListener).toHaveBeenCalledWith('close', expect.any(Function));

			expect(socket2.send).toHaveBeenCalledTimes(1);
			expect(socket2.send).toHaveBeenCalledWith(
				JSON.stringify({
					jsonrpc: '2.0',
					id: 2,
					method: 'subscribeExample',
					params: [],
				}),
			);

			await unsubscribe();
		});
	});
});
