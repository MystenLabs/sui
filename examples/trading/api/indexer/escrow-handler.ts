// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0
import { SuiEvent } from '@mysten/sui/client';
import { Prisma } from '@prisma/client';

import { prisma } from '../db';

type EscrowEvent = EscrowCreated | EscrowCancelled | EscrowSwapped;

type EscrowCreated = {
	sender: string;
	recipient: string;
	escrow_id: string;
	key_id: string;
	item_id: string;
};

type EscrowSwapped = {
	escrow_id: string;
};

type EscrowCancelled = {
	escrow_id: string;
};

/**
 * Handles all events emitted by the `escrow` module.
 * Data is modelled in a way that allows writing to the db in any order (DESC or ASC) without
 * resulting in data incosistencies.
 * We're constructing the updates to support multiple events involving a single record
 * as part of the same batch of events (but using a single write/record to the DB).
 * */
export const handleEscrowObjects = async (events: SuiEvent[], type: string) => {
	const updates: Record<string, Prisma.EscrowCreateInput> = {};

	for (const event of events) {
		if (!event.type.startsWith(type)) throw new Error('Invalid event module origin');
		const data = event.parsedJson as EscrowEvent;

		if (!Object.hasOwn(updates, data.escrow_id)) {
			updates[data.escrow_id] = {
				objectId: data.escrow_id,
			};
		}

		// Escrow cancellation case
		if (event.type.endsWith('::EscrowCancelled')) {
			const data = event.parsedJson as EscrowCancelled;
			updates[data.escrow_id].cancelled = true;
			continue;
		}

		// Escrow swap case
		if (event.type.endsWith('::EscrowSwapped')) {
			const data = event.parsedJson as EscrowSwapped;
			updates[data.escrow_id].swapped = true;
			continue;
		}

		const creationData = event.parsedJson as EscrowCreated;

		// Handle creation event
		updates[data.escrow_id].sender = creationData.sender;
		updates[data.escrow_id].recipient = creationData.recipient;
		updates[data.escrow_id].keyId = creationData.key_id;
		updates[data.escrow_id].itemId = creationData.item_id;
	}

	//  As part of the demo and to avoid having external dependencies, we use SQLite as our database.
	// 	Prisma + SQLite does not support bulk insertion & conflict handling, so we have to insert these 1 by 1
	// 	(resulting in multiple round-trips to the database).
	//  Always use a single `bulkInsert` query with proper `onConflict` handling in production databases (e.g Postgres)
	const promises = Object.values(updates).map((update) =>
		prisma.escrow.upsert({
			where: {
				objectId: update.objectId,
			},
			create: update,
			update,
		}),
	);
	await Promise.all(promises);
};
