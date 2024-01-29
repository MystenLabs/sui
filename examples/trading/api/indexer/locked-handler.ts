// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0
import { SuiEvent } from '@mysten/sui.js/client';
import { Locked } from '@prisma/client';

import { prisma } from '../db';

type Optional<T, K extends keyof T> = Pick<Partial<T>, K> & Omit<T, K>;

type LockEvent = LockCreated | LockDestroyed;

type LockCreated = {
	creator: string;
	lock_id: string;
	key_id: string;
};

type LockDestroyed = {
	lock_id: string;
};

/** Handles all events emitted by the `lock` module. */
export const handleLockObjects = async (events: SuiEvent[]) => {
	const updates: Record<string, Optional<Locked, 'id' | 'keyId' | 'deleted' | 'creator'>> = {};

	for (const event of events) {
		const data = event.parsedJson as LockEvent;
		const isDeletionEvent = !('key_id' in data);

		if (!Object.hasOwn(updates, data.lock_id)) {
			updates[data.lock_id] = {
				objectId: data.lock_id,
			};
		}

		// Handle deletion
		if (isDeletionEvent) {
			updates[data.lock_id].deleted = true;
			continue;
		}

		// Handle creation event
		updates[data.lock_id].keyId = data.key_id;
		updates[data.lock_id].creator = data.creator;
	}

	// SQLite does not support bulk insertion & conflict handling, so we have to insert 1 by 1.
	//  Always use a single `bulkInsert` query with proper `onConflict` handling in production.
	const promises = Object.values(updates).map((update) =>
		prisma.locked.upsert({
			where: {
				objectId: update.objectId,
			},
			create: {
				...update,
			},
			update,
		}),
	);
	await Promise.all(promises);
};
