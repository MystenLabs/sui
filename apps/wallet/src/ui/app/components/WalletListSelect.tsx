// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import { useAccounts } from '../hooks/useAccounts';
import { useDeriveNextAccountMutation } from '../hooks/useDeriveNextAccountMutation';
import { Link } from '../shared/Link';
import { SummaryCard } from './SummaryCard';
import { WalletListSelectItem } from './WalletListSelectItem';

export type WalletListSelectProps = {
    title: string;
    values: string[];
    onChange: (values: string[]) => void;
};

export function WalletListSelect({
    title,
    values,
    onChange,
}: WalletListSelectProps) {
    const accounts = useAccounts();
    const deriveNextAccount = useDeriveNextAccountMutation();
    return (
        <SummaryCard
            header={title}
            body={
                <ul className="flex flex-col items-stretch flex-1 p-0 m-0 self-stretch list-none">
                    {accounts.map(({ address }) => (
                        <li
                            key={address}
                            onClick={() => {
                                const newValues = [];
                                let found = false;
                                for (const anAddress of values) {
                                    if (anAddress === address) {
                                        found = true;
                                        continue;
                                    }
                                    newValues.push(anAddress);
                                }
                                if (!found) {
                                    newValues.push(address);
                                }
                                onChange(newValues);
                            }}
                        >
                            <WalletListSelectItem
                                address={address}
                                selected={values.includes(address)}
                            />
                        </li>
                    ))}
                </ul>
            }
            footer={
                <div className="flex flex-row flex-nowrap self-stretch justify-between">
                    <div>
                        <Link
                            color="heroDark"
                            weight="medium"
                            text="Select all"
                            onClick={() =>
                                onChange(accounts.map(({ address }) => address))
                            }
                        />
                    </div>
                    <div>
                        <Link
                            color="heroDark"
                            weight="medium"
                            text="New account"
                            loading={deriveNextAccount.isLoading}
                            onClick={() => deriveNextAccount.mutate()}
                        />
                    </div>
                </div>
            }
            minimalPadding
        />
    );
}
