// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import { EventId, SuiClient, SuiEvent, SuiEventFilter } from '@mysten/sui.js/client';

import { CONFIG } from '../config';
import { prisma } from '../db';
import { getClient } from '../sui-utils';
import { handleEscrowObjects } from './escrow-handler';
import { handleLockObjects } from './locked-handler';

type EventTracker = {
	// An move definition module in the format of `package::module`
	type: string;
	filter: SuiEventFilter;
	callback: (events: SuiEvent[]) => any;
};

const runningState: Record<string, boolean> = {};
const runningCursors: Record<string, EventId | undefined> = {};

const EVENTS_TO_TRACK: EventTracker[] = [
	{
		type: `${CONFIG.SWAP_CONTRACT.packageId}::lock`,
		filter: {
			MoveEventModule: {
				module: 'lock',
				package: CONFIG.SWAP_CONTRACT.packageId,
			},
		},
		callback: handleLockObjects,
	},
	{
		type: `${CONFIG.SWAP_CONTRACT.packageId}::shared`,
		filter: {
			MoveEventModule: {
				module: 'shared',
				package: CONFIG.SWAP_CONTRACT.packageId,
			},
		},
		callback: handleEscrowObjects,
	},
];

const runEventJob = async (client: SuiClient, tracker: EventTracker) => {
	// allow a single run per type at each time.
	if (runningState[tracker.type]) {
		return;
	}
	// mark as running so we can't re-enter
	runningState[tracker.type] = true;

	try {
		// our cursor to do pagination properly
		const cursor = (await getLatestCursor(tracker)) || undefined;

		// get the events from the chain.
		// For this implementation, we are going from start to finish.
		// This will also allow filling in a database from scratch!
		const { data, hasNextPage, nextCursor } = await client.queryEvents({
			query: tracker.filter,
			cursor,
			order: 'ascending',
		});

		// handle the data transformations defined for each event
		await tracker.callback(data);

		// We only update the cursor if we fetched extra data (which means there was a change).
		if (nextCursor && data.length > 0) await saveLatestCursor(tracker, nextCursor);
		runningState[tracker.type] = false;

		// we can speed up the polling if we know there are more events on the pipeline.
		if (hasNextPage) {
			runEventJob(client, tracker);
			return;
		}
	} catch (e) {
		console.error(e);
		// wrap everything in a catch statement to make sure we turn off running states.
		// We could also harden this further for better logging.
		runningState[tracker.type] = false;
	}
};

/// Gets the latest cursor for an event tracker, either from the DB (if it's undefined)
/// or from the running cursors.
const getLatestCursor = async (tracker: EventTracker) => {
	if (Object.hasOwn(runningCursors, tracker.type)) return runningCursors[tracker.type];

	const cursor = await prisma.cursor.findUnique({
		where: {
			id: tracker.type,
		},
	});

	runningCursors[tracker.type] = cursor || undefined;

	return runningCursors[tracker.type];
};

/// Saves the latest cursor for an event tracker to the db, so we can resume
/// from there.
const saveLatestCursor = async (tracker: EventTracker, cursor: EventId) => {
	const data = {
		eventSeq: cursor.eventSeq,
		txDigest: cursor.txDigest,
	};

	runningCursors[tracker.type] = cursor;

	return prisma.cursor.upsert({
		where: {
			id: tracker.type,
		},
		update: data,
		create: { id: tracker.type, ...data },
	});
};

/// Sets up all the listeners for the events we want to track.
/// They are polling the RPC endpoint every second.
export const setupListeners = () => {
	for (const event of EVENTS_TO_TRACK) {
		setInterval(() => {
			runEventJob(getClient(CONFIG.NETWORK), event);
		}, CONFIG.POLLING_INTERVAL_MS);
	}
};
