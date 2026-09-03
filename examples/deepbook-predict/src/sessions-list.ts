// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

// docs::#sessions-list
import type { SessionGrant } from '@mysten/deepbook-v3/sessions';
import { MAX_SESSIONS_PER_ACCOUNT, SessionsContract } from '@mysten/deepbook-v3/sessions';
import { Transaction } from '@mysten/sui/transactions';
import { client } from './client.js';
import { sessions, wrapperId } from './sessions-authorize.js';

// Every grant an account holds.
//
// There is no bulk onchain read: `session_expiration_ms` answers one address at a
// time. Listing means fetching the account's `DataKey<SessionsApp>` dynamic field
// and decoding it locally. Two details make or break this:
//
// 1. The field hangs off the derived ACCOUNT address, not off the wrapper. They
//    are different objects. `deriveSessionsFieldId` derives the account first,
//    then the field, so pass the owner address and let it do both steps.
// 2. `decodeSessions` takes the whole `Field<DataKey, SessionsData>` bytes that
//    `getObject` returns, not the inner value. It throws on truncated bytes
//    rather than decoding to an empty list, so an empty result means the account
//    really holds no grants.
//
// The field does not exist until the first `authorize_session`, so a missing
// object is an empty list rather than an error. Once attached it stays attached,
// even after every grant is revoked.
export async function listGrants(owner: string): Promise<SessionGrant[]> {
	const fieldId = sessions.deriveSessionsFieldId(owner);

	let content: Uint8Array | undefined;
	try {
		const { object } = await client.core.getObject({
			objectId: fieldId,
			include: { content: true },
		});
		content = object.content;
	} catch {
		// No sessions data attached to this account yet.
		return [];
	}
	if (!content) return [];

	return SessionsContract.decodeSessions(content);
}

// Split a listing at the current time. Use the helpers rather than comparing by
// hand: the chain asserts `now < expiresAtMs`, so a grant is dead AT its expiry,
// and a filter written as `nowMs > expiresAtMs` leaves the grant expiring exactly
// at `nowMs` occupying a slot forever.
export async function grantsByState(
	owner: string,
	nowMs: number = Date.now(),
): Promise<{ active: SessionGrant[]; expired: SessionGrant[]; slotsFree: number }> {
	const grants = await listGrants(owner);
	return {
		active: SessionsContract.activeSessions(grants, nowMs),
		expired: SessionsContract.expiredSessions(grants, nowMs),
		// Expired grants still occupy slots, so free slots count against every
		// stored address, not just the live ones.
		slotsFree: MAX_SESSIONS_PER_ACCOUNT - grants.length,
	};
}

// Reclaim the slots expired grants are still holding.
//
// Time passing executes no Move code, so nothing prunes expired entries. They
// count toward the 20 stored addresses until the owner revokes them, and the
// twenty-first `authorize_session` aborts `ESessionLimitExceeded` even when every
// stored grant is long dead. Run this before granting on a busy account.
//
// Returns null when there is nothing to reclaim, so the caller does not sign an
// empty transaction.
export async function reclaimExpiredSlots(
	owner: string,
	nowMs: number = Date.now(),
): Promise<Transaction | null> {
	const { expired } = await grantsByState(owner, nowMs);
	if (expired.length === 0) return null;

	const tx = new Transaction();
	tx.setSender(owner);
	for (const grant of expired) {
		tx.add(sessions.revokeSession({ wrapperId: wrapperId(owner), session: grant.session }));
	}
	return tx;
}
// docs::/#sessions-list
