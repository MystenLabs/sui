// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

// docs::#sessions-authorize
import {
	MAX_SESSION_DURATION_MS,
	SessionsContract,
	getSessionsConfig,
} from '@mysten/deepbook-v3/sessions';
import { Transaction } from '@mysten/sui/transactions';
import { NETWORK } from './config.js';

// One contract instance drives every sessions call in these examples.
// `getSessionsConfig` returns the deployed sessions package, the shared
// `SessionsConfig` object, the account registry the app is authorized against,
// the Predict protocol config, and the two extra IDs the DeepBook spot wrappers
// need. Sessions is Testnet only, so any other network throws.
export const SESSIONS_CONFIG = getSessionsConfig(NETWORK);

export const sessions = new SessionsContract(SESSIONS_CONFIG);

// The owner's shared `AccountWrapper` ID, derived from the owner address with no
// chain read. Every builder below takes it as `wrapperId`.
export function wrapperId(owner: string): string {
	return sessions.deriveAccountWrapperId(owner);
}

// Authorize `session` to trade the owner's account until `now + durationMs`.
//
// A grant is trading authority over the whole account, not a scoped permission.
// The session key cannot withdraw to an address, cannot grant or revoke
// sessions, and cannot outlive its expiry, but within those limits it chooses
// the market, the size, and the price bounds, and the spot wrappers reach the
// account's entire Base, Quote, and DEEP balance. Fund an account that hands out
// ephemeral session keys with only what you are willing to put at risk.
//
// The owner must sign, because the contract derives owner authority from the
// transaction sender. Re-authorizing an address that already holds a grant
// replaces its expiry in place and consumes no additional slot.
export function authorizeSession(params: {
	owner: string;
	session: string;
	durationMs: number;
}): Transaction {
	const { owner, session, durationMs } = params;

	// The chain asserts the same bounds and aborts `EInvalidSessionDuration`.
	// Checking here turns a wasted transaction into a local error.
	if (durationMs <= 0 || durationMs > MAX_SESSION_DURATION_MS) {
		throw new Error(
			`durationMs must be greater than 0 and at most ${MAX_SESSION_DURATION_MS} (30 days), got ${durationMs}.`,
		);
	}

	const tx = new Transaction();
	tx.setSender(owner);
	tx.add(sessions.authorizeSession({ wrapperId: wrapperId(owner), session, durationMs }));

	// Ready to sign with the owner's key. Nothing here signs it.
	return tx;
}

// A short-lived grant for one trading run. Expiry is computed from the onchain
// clock at execution time, not from when you build the transaction, so a queued
// or retried transaction still gets its full duration.
export function authorizeForOneHour(owner: string, session: string): Transaction {
	return authorizeSession({ owner, session, durationMs: 60 * 60 * 1000 });
}
// docs::/#sessions-authorize
