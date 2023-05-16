// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import { Search16 } from '@mysten/icons';
import { useMemo, useState } from 'react';

import { useFetchQredoAccounts } from '../hooks';
import { QredoAccountItem } from './QredoAccountItem';
import { SummaryCard } from '_components/SummaryCard';
import Alert from '_components/alert';
import Loading from '_components/loading';
import { type Wallet } from '_src/shared/qredo-api';
import { Link } from '_src/ui/app/shared/Link';

function matchesSearchTerm(
    { walletID, address, labels }: Wallet,
    searchTerm: string
) {
    const term = searchTerm.trim().toLowerCase();
    return (
        !term ||
        walletID.toLowerCase().includes(term) ||
        address.toLowerCase().includes(term) ||
        labels.some(({ value }) => value.toLowerCase().includes(term))
    );
}

export type SelectQredoAccountsSummaryCardProps = {
    qredoID: string;
    fetchAccountsEnabled: boolean;
    selectedAccounts: Wallet[];
    onChange: (selectedAccounts: Wallet[]) => void;
};

export function SelectQredoAccountsSummaryCard({
    qredoID,
    fetchAccountsEnabled = false,
    selectedAccounts,
    onChange,
}: SelectQredoAccountsSummaryCardProps) {
    const selectedAccountsIndex = useMemo(
        () =>
            selectedAccounts.reduce<Record<string, boolean>>(
                (acc, { walletID }) => {
                    acc[walletID] = true;
                    return acc;
                },
                {}
            ),
        [selectedAccounts]
    );
    const { data, isLoading, error } = useFetchQredoAccounts(
        qredoID,
        fetchAccountsEnabled
    );
    const [searchTerm, setSearchTerm] = useState('');
    return (
        <SummaryCard
            header="Select accounts"
            body={
                <Loading loading={isLoading}>
                    {error ? (
                        <Alert>
                            Failed to fetch accounts. Please try again later.
                        </Alert>
                    ) : data?.length ? (
                        <>
                            <div className="flex items-center bg-white pt-1 sticky -top-4">
                                <input
                                    className="flex-1 p-3 pr-7.5 bg-white border border-solid border-gray-45 rounded-lg text-steel-dark placeholder:text-steel"
                                    onChange={(e) =>
                                        setSearchTerm(e.target.value)
                                    }
                                    value={searchTerm}
                                    placeholder="Search"
                                />
                                <Search16 className="absolute w-4.5 h-4.5 right-3 pointer-events-none text-steel" />
                            </div>
                            <div className="divide-x-0 divide-y divide-gray-40 divide-solid">
                                {data
                                    .filter((aWallet) =>
                                        matchesSearchTerm(aWallet, searchTerm)
                                    )
                                    .map((wallet) => (
                                        <QredoAccountItem
                                            key={wallet.walletID}
                                            {...wallet}
                                            selected={
                                                !!selectedAccountsIndex[
                                                    wallet.walletID
                                                ]
                                            }
                                            onClick={() => {
                                                const newSelected =
                                                    !selectedAccountsIndex[
                                                        wallet.walletID
                                                    ];
                                                onChange(
                                                    data.filter(
                                                        ({ walletID }) =>
                                                            walletID ===
                                                            wallet.walletID
                                                                ? newSelected
                                                                : !!selectedAccountsIndex[
                                                                      walletID
                                                                  ]
                                                    )
                                                );
                                            }}
                                        />
                                    ))}
                            </div>
                        </>
                    ) : (
                        <Alert>No accounts found</Alert>
                    )}
                </Loading>
            }
            footer={
                <div className="flex items-center justify-center">
                    <Link
                        text="Select All Accounts"
                        color="heroDark"
                        weight="medium"
                        size="bodySmall"
                        onClick={() => {
                            if (data) {
                                onChange([...data]);
                            }
                        }}
                        disabled={!data?.length}
                    />
                </div>
            }
        />
    );
}
