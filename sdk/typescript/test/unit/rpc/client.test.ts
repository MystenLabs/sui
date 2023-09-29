// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import { rest } from 'msw';
import { setupServer } from 'msw/node';
import { afterAll, afterEach, beforeAll, describe, expect, it, vi } from 'vitest';

import { SuiObjectData } from '../../../src/client';
import { JsonRpcClient } from '../../../src/rpc/client';
import { GetOwnedObjectsResponse } from '../../../src/types';

const EXAMPLE_OBJECT: SuiObjectData = {
	objectId: '8dc6a6f70564e29a01c7293a9c03818fda2d049f',
	version: '0',
	digest: 'CI8Sf+t3Xrt5h9ENlmyR8bbMVfg6df3vSDc08Gbk9/g=',
	owner: {
		AddressOwner: '0x215592226abfec8d03fbbeb8b30eb0d2129c94b0',
	},
	type: 'moveObject',
	previousTransaction: '4RJfkN9SgLYdb0LqxBHh6lfRPicQ8FLJgzi9w2COcTo=',
};

const OBJECT_WITH_WRONG_SCHEMA = {
	objectId: '8dc6a6f70564e29a01c7293a9c03818fda2d049f',
	version: 0,
	digest: 'CI8Sf+t3Xrt5h9ENlmyR8bbMVfg6df3vSDc08Gbk9/g=',
	owner: {
		AddressOwner1: '0x215592226abfec8d03fbbeb8b30eb0d2129c94b0',
	},
	type: 'moveObject',
	previousTransaction: '4RJfkN9SgLYdb0LqxBHh6lfRPicQ8FLJgzi9w2COcTo=',
};

const MOCK_ENDPOINT = 'http://127.0.0.1:9000/';

const server = setupServer(
	rest.post('http://127.0.0.1:9000/', async (req, res, ctx) => {
		const body = await req.json();
		return res(
			ctx.status(200),
			ctx.json({
				jsonrpc: '2.0',
				id: body.id,
				result: body.params[0] === '0xfail' ? [OBJECT_WITH_WRONG_SCHEMA] : [EXAMPLE_OBJECT],
			}),
		);
	}),
);

describe('JSON-RPC Client', () => {
	beforeAll(() => server.listen({ onUnhandledRequest: 'error' }));
	afterAll(() => server.close());
	afterEach(() => {
		server.resetHandlers();
		vi.restoreAllMocks();
	});

	it('requestWithType', async () => {
		const client = new JsonRpcClient(MOCK_ENDPOINT);
		const resp = await client.requestWithType(
			'sui_getOwnedObjectsByAddress',
			['0xsuccess'],
			GetOwnedObjectsResponse,
		);
		expect(resp).toHaveLength(1);
		expect(resp[0]).toEqual(EXAMPLE_OBJECT);
	});

	it('requestWithType should throw in tests', async () => {
		const client = new JsonRpcClient(MOCK_ENDPOINT);

		// NOTE: We set `console.warn` to throw in tests, so we can catch it here.
		await expect(
			client.requestWithType('sui_getOwnedObjectsByAddress', ['0xfail'], GetOwnedObjectsResponse),
		).rejects.toThrowError();
	});

	describe('outside of tests', () => {
		beforeAll(() => {
			process.env.NODE_ENV = 'production';
		});

		afterAll(() => {
			process.env.NODE_ENV = 'test';
		});

		it('requestWithType should not throw', async () => {
			process.env.NODE_ENV = 'production';
			const client = new JsonRpcClient(MOCK_ENDPOINT);

			const result = await client.requestWithType(
				'sui_getOwnedObjectsByAddress',
				['0xfail'],
				GetOwnedObjectsResponse,
			);

			expect(result[0].type).toEqual('moveObject');
		});
	});
});
