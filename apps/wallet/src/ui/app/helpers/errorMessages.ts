// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import { LockedDeviceError, StatusCodes } from '@ledgerhq/errors';

import {
    isLedgerTransportStatusError,
    LedgerConnectionFailedError,
    LedgerDeviceNotFoundError,
    LedgerNoTransportMechanismError,
} from '../components/ledger/ledgerErrors';

/**
 * Helper method for producing user-friendly error messages from Signer operations
 * from SignerWithProvider instances (e.g., signTransaction, getAddress, and so forth)
 */
export function getSignerOperationErrorMessage(error: unknown) {
    return (
        getLedgerConnectionErrorMessage(error) ||
        getSuiApplicationErrorMessage(error) ||
        (error as Error).message ||
        'Something went wrong.'
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
        return 'Connect your Ledger device and try again.';
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
    } else if (isLedgerTransportStatusError(error)) {
        if (error.statusCode === StatusCodes.INS_NOT_SUPPORTED) {
            return "Something went wrong. We're working on it!";
        } else {
            return 'Make sure the Sui app is open on your device.';
        }
    }
    return null;
}
