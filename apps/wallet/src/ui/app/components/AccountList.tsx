// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import { useAccounts } from '../hooks/useAccounts';
import { AccountListItem, type AccountItemProps } from './AccountListItem';

export type AccountListProps = {
    onAccountSelected: AccountItemProps['onAccountSelected'];
};

export function AccountList({ onAccountSelected }: AccountListProps) {
    const allAccounts = useAccounts();
    return (
        <div className="flex flex-col items-stretch">
            {allAccounts.map(({ address }) => (
                <AccountListItem
                    address={address}
                    key={address}
                    onAccountSelected={onAccountSelected}
                />
            ))}
        </div>
    );
}
