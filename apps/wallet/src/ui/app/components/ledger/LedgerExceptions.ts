// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import { LockedDeviceError } from '@ledgerhq/errors';

import { reportSentryError } from '_src/shared/sentry';

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
        `Unable to connect to the user's Ledger device: ${errorMessage}`
    );
}

/**
 * Helper method for producing user-friendly error messages from Ledger connection errors
 */
export function getLedgerConnectionErrorMessage(error: unknown) {
    if (error instanceof LedgerConnectionFailedError) {
        return 'Ledger connection failed. Try again.';
    } else if (error instanceof LedgerNoTransportMechanismError) {
        return "Your browser unfortunately doesn't support USB or HID.";
    } else if (error instanceof LedgerDeviceNotFoundError) {
        return 'Connect your Ledger device and open the Sui app.';
    } else if (error instanceof LockedDeviceError) {
        return 'Your device is locked. Unlock it and try again.';
    }
    return null;
}

/**
 * Helper method for producing user-friendly error messages from errors that arise from
 * operations on the Sui Ledger application
 */
export function getSuiApplicationErrorMessage(error: unknown) {
    if (error instanceof LockedDeviceError) {
        return 'Your device is locked. Unlock it and try again.';
    }
    // When something goes wrong in the Sui application itself, a TransportStatusError is
    // thrown. Unfortunately, @ledgerhq/errors doesn't expose this error in the form of a
    // custom Error class. This makes identification of what went wrong an ugly process, so
    // for now we'll opt to display a error message that's appropriate about 90% of the time
    return 'Make sure the Sui app is open on your device.';
}
