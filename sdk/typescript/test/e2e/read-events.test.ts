// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import { describe, it, expect, beforeAll } from 'vitest';
import { setup, TestToolbox } from './utils/setup';

describe('Event Reading API', () => {
	let toolbox: TestToolbox;

	beforeAll(async () => {
		toolbox = await setup();
	});

	it('Get All Events', async () => {
		// TODO: refactor so that we can provide None here to signify there's no filter
		const allEvents = await toolbox.client.queryEvents({
			query: { All: [] },
		});
		expect(allEvents.data.length).to.greaterThan(0);
	});

	it('Get all event paged', async () => {
		const page1 = await toolbox.client.queryEvents({
			query: { All: [] },
			limit: 2,
		});
		expect(page1.nextCursor).to.not.equal(null);
	});

	it('Get events by sender paginated', async () => {
		const query1 = await toolbox.client.queryEvents({
			query: { Sender: toolbox.address() },
			limit: 2,
		});
		expect(query1.data.length).toEqual(0);
	});
});
