// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import { LedgerAccountItem, type LedgerAccount } from './LedgerAccountItem';

type LedgerAccountListProps = {
    accounts: LedgerAccount[];
    onAccountClick: (account: LedgerAccount) => void;
};

export function LedgerAccountList({
    accounts,
    onAccountClick,
}: LedgerAccountListProps) {
    return (
        <ul className="list-none m-0 p-0">
            {accounts.map((account) => (
                <li className="pt-2 pb-2 first:pt-1" key={account.address}>
                    <button
                        className="w-full appearance-none border-0 p-0 bg-transparent cursor-pointer"
                        onClick={() => {
                            onAccountClick(account);
                        }}
                    >
                        <LedgerAccountItem
                            isSelected={account.isSelected}
                            address={account.address}
                        />
                    </button>
                </li>
            ))}
        </ul>
    );
}
