// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import { useNextMenuUrl } from '../hooks';
import { VerifyLedgerConnectionStatus } from './VerifyLedgerConnectionStatus';
import {
    AccountType,
    type SerializedAccount,
} from '_src/background/keyring/Account';
import { Link } from '_src/ui/app/shared/Link';

export type AccountActionsProps = {
    account: SerializedAccount;
};

export function AccountActions({ account }: AccountActionsProps) {
    const exportAccountUrl = useNextMenuUrl(true, `/export/${account.address}`);

    let actionContent: JSX.Element | null = null;
    switch (account.type) {
        case AccountType.LEDGER:
            actionContent = (
                <VerifyLedgerConnectionStatus
                    accountAddress={account.address}
                    derivationPath={account.derivationPath}
                />
            );
            break;
        case AccountType.IMPORTED:
        case AccountType.DERIVED:
            actionContent = (
                <Link
                    text="Export Private Key"
                    to={exportAccountUrl}
                    color="heroDark"
                    weight="medium"
                />
            );
            break;
        default:
            throw new Error(`Encountered unknown account type`);
    }

    return (
        <div className="flex flex-row flex-nowrap items-center flex-1">
            <div>{actionContent}</div>
        </div>
    );
}
