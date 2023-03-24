// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import { LockedDeviceError } from '@ledgerhq/errors';
import toast from 'react-hot-toast';
import { useNavigate } from 'react-router-dom';

import { useNextMenuUrl } from '../menu/hooks';
import { ConnectLedgerModal } from './ConnectLedgerModal';
import {
    LedgerConnectionFailedError,
    LedgerNoTransportMechanismError,
} from './LedgerExceptions';

export function ConnectLedgerModalContainer() {
    const navigate = useNavigate();
    const accountsUrl = useNextMenuUrl(true, '/accounts');
    const importLedgerAccountsUrl = useNextMenuUrl(
        true,
        '/import-ledger-accounts'
    );

    return (
        <ConnectLedgerModal
            onClose={() => {
                navigate(accountsUrl);
            }}
            onError={(error) => {
                navigate(accountsUrl);
                toast.error(getLedgerErrorMessage(error));
            }}
            onConfirm={() => {
                navigate(importLedgerAccountsUrl);
            }}
        />
    );
}

function getLedgerErrorMessage(error: unknown) {
    if (error instanceof LockedDeviceError) {
        return 'Your device is locked. Un-lock it and try again.';
    } else if (error instanceof LedgerConnectionFailedError) {
        return 'Ledger connection failed.';
    } else if (error instanceof LedgerNoTransportMechanismError) {
        return "Your machine doesn't support USB or HID.";
    }
    return 'Something went wrong. Try again.';
}
