// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

interface LedgerTransportStatusError extends Error {
	name: 'TransportStatusError';
	statusCode: number;
}

export class LedgerConnectionFailedError extends Error {
	constructor(message: string) {
		super(message);
		Object.setPrototypeOf(this, LedgerConnectionFailedError.prototype);
	}
}

export class LedgerNoTransportMechanismError extends Error {
	constructor(message: string) {
		super(message);
		Object.setPrototypeOf(this, LedgerNoTransportMechanismError.prototype);
	}
}

export class LedgerDeviceNotFoundError extends Error {
	constructor(message: string) {
		super(message);
		Object.setPrototypeOf(this, LedgerDeviceNotFoundError.prototype);
	}
}

// Connection methods from the Ledger transport libraries don't throw well-structured
// errors, so we can use this utility to form more explicit and structured errors
export function convertErrorToLedgerConnectionFailedError(error: unknown) {
	const errorMessage = error instanceof Error ? error.message : String(error);
	return new LedgerConnectionFailedError(
		`Unable to connect to the user's Ledger device: ${errorMessage}`,
	);
}

// When something goes wrong in the Sui application itself, a TransportStatusError is
// thrown. Unfortunately, @ledgerhq/errors doesn't expose this error in the form of a
// custom Error class. This makes identification of what went wrong less straightforward
export function isLedgerTransportStatusError(error: unknown): error is LedgerTransportStatusError {
	return error instanceof Error && error.name === 'TransportStatusError';
}
