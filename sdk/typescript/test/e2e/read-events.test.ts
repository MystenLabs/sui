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
    const allEvents = await toolbox.provider.getEvents('All', null, null);
    expect(allEvents.data.length).to.greaterThan(0);
  });

  it('Get all event paged', async () => {
    const page1 = await toolbox.provider.getEvents('All', null, 2);
    expect(page1.nextCursor).to.not.equal(null);
  });

  it('Get events by sender paginated', async () => {
    const query1 = await toolbox.provider.getEvents(
      { Sender: toolbox.address() },
      null,
      2
    );
    expect(query1.data.length).toEqual(0);
  });

  it('Get events by recipient paginated', async () => {
    const query2 = await toolbox.provider.getEvents(
      { Recipient: { AddressOwner: toolbox.address() } },
      null,
      2
    );
    expect(query2.data.length).toEqual(2);
  });
});
