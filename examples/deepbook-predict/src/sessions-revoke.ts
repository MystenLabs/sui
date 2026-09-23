// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

// docs::#sessions-revoke
import { bcs } from '@mysten/sui/bcs';
import { Transaction } from '@mysten/sui/transactions';
import { client } from './client.js';
import { sessions, wrapperId } from './sessions-authorize.js';

// Revoke one grant. The owner signs, the slot frees immediately, and the address
// stops being able to trade from the next transaction on.
//
// Revocation takes no `SessionsConfig` and is not version gated, so it keeps
// working after the sessions package is retired by a version bump. It does not
// unwind anything the session already did: positions, resting spot orders, and
// executed trades all survive.
export function revokeSession(owner: string, session: string): Transaction {
	const tx = new Transaction();
	tx.setSender(owner);
	tx.add(sessions.revokeSession({ wrapperId: wrapperId(owner), session }));
	return tx;
}

// Read one address's absolute expiry, in milliseconds since the epoch.
//
// `session_expiration_ms` returns `Option<u64>`: `none` for an address that was
// never granted or has been revoked, and the stored timestamp otherwise. It does
// not classify the timestamp, so compare it with the current time yourself, and
// remember the comparison is strict: the grant is dead AT `expiresAtMs`.
//
// The call is a `public fun` with a return value rather than an `entry` call, so
// simulate it and decode the returned BCS instead of executing it.
export async function sessionExpirationMs(
	owner: string,
	session: string,
): Promise<bigint | null> {
	const tx = new Transaction();
	// A simulation needs a sender; any address works for a read.
	tx.setSender(owner);
	tx.add(sessions.sessionExpirationMs({ wrapperId: wrapperId(owner), session }));

	const result = await client.core.simulateTransaction({
		transaction: tx,
		// Public non-entry functions are only inspectable with checks disabled.
		checksEnabled: false,
		include: { commandResults: true },
	});
	if (result.$kind === 'FailedTransaction') {
		throw new Error(`session_expiration_ms simulation failed for ${session}.`);
	}

	const returned = result.commandResults?.[0]?.returnValues?.[0]?.bcs;
	if (!returned) throw new Error('simulateTransaction returned no command results.');

	const expiry = bcs.option(bcs.u64()).parse(returned);
	return expiry === null ? null : BigInt(expiry);
}

// Whether `session` can trade right now. `false` covers both an address that
// never held a grant and one whose grant has expired or been revoked.
export async function isSessionActive(
	owner: string,
	session: string,
	nowMs: number = Date.now(),
): Promise<boolean> {
	const expiry = await sessionExpirationMs(owner, session);
	return expiry !== null && BigInt(nowMs) < expiry;
}

// Confirm a revocation by reading, because the transaction result cannot.
//
// Revoking an address that holds no grant is a silent no-op: it does not abort
// and it emits no `SessionRevoked` event, so a successful transaction proves
// nothing about whether a grant was removed or was never there. Read before and
// after when the difference matters, for example when you are auditing a key you
// believe you granted.
export async function revokeAndConfirm(
	owner: string,
	session: string,
): Promise<{ tx: Transaction; hadGrant: boolean }> {
	const before = await sessionExpirationMs(owner, session);
	return { tx: revokeSession(owner, session), hadGrant: before !== null };
}
// docs::/#sessions-revoke
