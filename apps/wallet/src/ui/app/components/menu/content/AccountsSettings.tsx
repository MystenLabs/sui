// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import { useFeature } from '@growthbook/growthbook-react';
import { LockedDeviceError } from '@ledgerhq/errors';
import { LockLocked16 as LockedLockIcon } from '@mysten/icons';
import { useState } from 'react';
import toast from 'react-hot-toast';
import { useNavigate } from 'react-router-dom';

import { ConnectLedgerModal } from '../../ledger/ConnectLedgerModal';
import {
    LedgerConnectionFailedError,
    LedgerNoTransportMechanismError,
} from '../../ledger/LedgerExceptions';
import { Account } from './Account';
import { MenuLayout } from './MenuLayout';
import { useNextMenuUrl } from '_components/menu/hooks';
import { FEATURES } from '_src/shared/experimentation/features';
import { useAccounts } from '_src/ui/app/hooks/useAccounts';
import { useDeriveNextAccountMutation } from '_src/ui/app/hooks/useDeriveNextAccountMutation';
import { Button } from '_src/ui/app/shared/ButtonUI';

export function AccountsSettings() {
    const backUrl = useNextMenuUrl(true, '/');
    const importPrivateKeyUrl = useNextMenuUrl(true, '/import-private-key');
    const accounts = useAccounts();
    const isMultiAccountsEnabled = useFeature(
        FEATURES.WALLET_MULTI_ACCOUNTS
    ).on;
    const createAccountMutation = useDeriveNextAccountMutation();

    const [isConnectLedgerModalOpen, setConnectLedgerModalOpen] =
        useState(false);

    const { on: isLedgerIntegrationEnabled } = useFeature(
        FEATURES.WALLET_LEDGER_INTEGRATION
    );

    const navigate = useNavigate();
    const importLedgerAccountsUrl = useNextMenuUrl(
        true,
        '/import-ledger-accounts'
    );

    return (
        <MenuLayout title="Accounts" back={backUrl}>
            <div className="flex flex-col gap-3">
                {accounts.map(({ address }) => (
                    <Account address={address} key={address} />
                ))}
                {isMultiAccountsEnabled ? (
                    <>
                        <Button
                            variant="outline"
                            size="tall"
                            text="Create New Account"
                            loading={createAccountMutation.isLoading}
                            onClick={() => createAccountMutation.mutate()}
                        />
                        <Button
                            variant="outline"
                            size="tall"
                            text="Import Private Key"
                            to={importPrivateKeyUrl}
                        />
                    </>
                ) : null}
                {isLedgerIntegrationEnabled ? (
                    <>
                        <Button
                            variant="outline"
                            size="tall"
                            text="Connect Ledger Wallet"
                            before={<LockedLockIcon />}
                            onClick={() => setConnectLedgerModalOpen(true)}
                        />
                        <ConnectLedgerModal
                            isOpen={isConnectLedgerModalOpen}
                            onClose={() => setConnectLedgerModalOpen(false)}
                            onError={(error) => {
                                setConnectLedgerModalOpen(false);
                                toast.error(getLedgerErrorMessage(error));
                            }}
                            onConfirm={() => {
                                setConnectLedgerModalOpen(false);
                                navigate(importLedgerAccountsUrl);
                            }}
                        />
                    </>
                ) : null}
            </div>
        </MenuLayout>
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
    return 'Make sure the Sui application is open and try again.';
}
