// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import { beforeAll, describe, expect, it } from 'vitest';

import { setup, TestToolbox } from './utils/setup';

describe('Event Reading API', () => {
	let toolbox: TestToolbox;

	beforeAll(async () => {
		toolbox = await setup();

		// Wait for next epoch to ensure there are events
		await new Promise((resolve) => setTimeout(resolve, 60_000));
	});

	it('Get All Events', async () => {
		// TODO: refactor so that we can provide None here to signify there's no filter
		const allEvents = await toolbox.client.queryEvents({
			query: { TimeRange: { startTime: '0', endTime: Date.now().toString() } },
		});
		expect(allEvents.data.length).to.greaterThan(0);
	});

	it('Get all event paged', async () => {
		const page1 = await toolbox.client.queryEvents({
			query: { TimeRange: { startTime: '0', endTime: Date.now().toString() } },
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
