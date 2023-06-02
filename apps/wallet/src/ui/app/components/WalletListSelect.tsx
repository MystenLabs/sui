// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import { cx } from 'class-variance-authority';
import { useMemo, useState } from 'react';

import { useAccounts } from '../hooks/useAccounts';
import { SummaryCard } from './SummaryCard';
import {
    WalletListSelectItem,
    type WalletListSelectItemProps,
} from './WalletListSelectItem';

export type WalletListSelectProps = {
    title: string;
    values: string[];
    visibleValues?: string[];
    mode?: WalletListSelectItemProps['mode'];
    disabled?: boolean;
    onChange: (values: string[]) => void;
};

export function WalletListSelect({
    title,
    values,
    visibleValues,
    mode = 'select',
    disabled = false,
    onChange,
}: WalletListSelectProps) {
    const [newAccounts] = useState<string[]>([]);
    const accounts = useAccounts();
    const filteredAccounts = useMemo(() => {
        if (visibleValues) {
            return accounts.filter(({ address }) =>
                visibleValues.includes(address)
            );
        }
        return accounts;
    }, [accounts, visibleValues]);
    return (
        <SummaryCard
            header={title}
            body={
                <ul
                    className={cx(
                        'flex flex-col items-stretch flex-1 p-0 m-0 self-stretch list-none',
                        disabled ? 'opacity-70' : ''
                    )}
                >
                    {filteredAccounts.map(({ address }) => (
                        <li
                            key={address}
                            onClick={() => {
                                if (disabled) {
                                    return;
                                }
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
                                mode={mode}
                                disabled={disabled}
                                isNew={newAccounts.includes(address)}
                            />
                        </li>
                    ))}
                </ul>
            }
            minimalPadding
        />
    );
}
