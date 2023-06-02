// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import { useCopyToClipboard } from '@mysten/core';
import { Copy12, X12 } from '@mysten/icons';
import { useState, type ReactNode } from 'react';
import { toast } from 'react-hot-toast';

import { BadgeLabel } from '../../BadgeLabel';
import { useNextMenuUrl } from '../hooks';
import { VerifyLedgerConnectionStatus } from './VerifyLedgerConnectionStatus';
import {
    AccountType,
    type SerializedAccount,
} from '_src/background/keyring/Account';
import { Link } from '_src/ui/app/shared/Link';
import { Text } from '_src/ui/app/shared/text';

export type AccountActionsProps = {
    account: SerializedAccount;
};

export function AccountActions({ account }: AccountActionsProps) {
    const exportAccountUrl = useNextMenuUrl(true, `/export/${account.address}`);
    const recoveryPassphraseUrl = useNextMenuUrl(true, '/recovery-passphrase');
    const [pinToastID, setPinToastID] = useState<string | null>(null);
    const copyToClipboard = useCopyToClipboard();

    let actionContent: ReactNode | null = null;
    switch (account.type) {
        case AccountType.LEDGER:
            actionContent = (
                <div>
                    <VerifyLedgerConnectionStatus
                        accountAddress={account.address}
                        derivationPath={account.derivationPath}
                    />
                </div>
            );
            break;
        case AccountType.IMPORTED:
            actionContent = (
                <div>
                    <Link
                        text="Export Private Key"
                        to={exportAccountUrl}
                        color="heroDark"
                        weight="medium"
                    />
                </div>
            );
            break;
        case AccountType.DERIVED:
            actionContent = (
                <>
                    <div>
                        <Link
                            text="Export Private Key"
                            to={exportAccountUrl}
                            color="heroDark"
                            weight="medium"
                        />
                    </div>
                    <div>
                        <Link
                            to={recoveryPassphraseUrl}
                            color="heroDark"
                            weight="medium"
                            text="Export Passphrase"
                        />
                    </div>
                </>
            );
            break;
        case AccountType.QREDO:
            actionContent = account.labels?.length
                ? account.labels.map(({ name, value }) => (
                      <BadgeLabel label={value} key={name} />
                  ))
                : null;
            break;
        case AccountType.ZK:
            actionContent = (
                <div>
                    <Link
                        disabled={!!pinToastID}
                        onClick={(t) => {
                            if (!pinToastID) {
                                const tID = toast(
                                    (t) => (
                                        <div className="relative flex flex-col gap-4">
                                            <X12
                                                className="absolute top-0 right-0 cursor-pointer"
                                                onClick={() => {
                                                    toast.dismiss(t.id);
                                                    setPinToastID(null);
                                                }}
                                            />
                                            <Text
                                                variant="captionSmall"
                                                weight="semibold"
                                            >
                                                Your account pin is
                                            </Text>
                                            <Text
                                                variant="pSubtitleSmall"
                                                mono
                                                weight="bold"
                                            >
                                                {account.pin}{' '}
                                                <Copy12
                                                    className="cursor-pointer"
                                                    onClick={async () => {
                                                        await copyToClipboard(
                                                            account.pin
                                                        );
                                                        setPinToastID(null);
                                                        toast.dismiss(t.id);
                                                        toast.success(
                                                            'Address pin copied'
                                                        );
                                                    }}
                                                />
                                            </Text>
                                        </div>
                                    ),
                                    {
                                        duration: Infinity,
                                    }
                                );
                                setPinToastID(tID);
                            }
                        }}
                        text="Show Pin"
                        color="heroDark"
                        weight="medium"
                    />
                </div>
            );
            break;
        default:
            throw new Error(`Encountered unknown account type`);
    }

    return (
        <div className="flex items-center flex-1 gap-4 pb-1 overflow-x-auto">
            {actionContent}
        </div>
    );
}
