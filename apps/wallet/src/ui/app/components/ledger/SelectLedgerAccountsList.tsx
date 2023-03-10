// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import { LedgerAccountItem, type LedgerAccount } from './LedgerAccountItem';

type SelectLedgerAccountsListProps = {
    accounts: LedgerAccount[];
    onSelect: (selectedAccount: LedgerAccount) => void;
};

export function SelectLedgerAccountsList({
    accounts,
    onSelect,
}: SelectLedgerAccountsListProps) {
    return (
        <ul className="list-none h-[272px] m-0 p-0 -mr-2 mt-1 py-0 pr-2 overflow-auto custom-scrollbar">
            {accounts.map((account) => {
                return (
                    <li className="pt-2 pb-2 first:pt-1" key={account.address}>
                        <button
                            className="w-full appearance-none border-0 p-0 bg-transparent cursor-pointer"
                            onClick={() => {
                                onSelect(account);
                            }}
                        >
                            <LedgerAccountItem
                                isSelected={account.isSelected}
                                address={account.address}
                            />
                        </button>
                    </li>
                );
            })}
        </ul>
    );
}
