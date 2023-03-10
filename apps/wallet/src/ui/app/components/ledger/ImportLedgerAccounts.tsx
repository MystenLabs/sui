// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import { LockUnlocked16 as UnlockedLockIcon } from '@mysten/icons';
<<<<<<< HEAD
import { Navigate, useNavigate } from 'react-router-dom';

import { SummaryCard } from '../SummaryCard';
import { useNextMenuUrl } from '../menu/hooks';
import Overlay from '../overlay';
import { LedgerAccount } from './LedgerAccount';
import { useSuiLedgerClient } from '_src/ui/app/components/ledger/SuiLedgerClientProvider';
import { Button } from '_src/ui/app/shared/ButtonUI';
import { Link } from '_src/ui/app/shared/Link';

const mockAccounts = [
    {
        isSelected: false,
        address:
            '0x7a286c8455a801f6d81faaa0f87543fa4a0de64dcc48b9c9308ee18f0f6ccdd3',
        balance: 30,
    },
    {
        isSelected: true,
        address:
            '0x7a286c8455a401f6d81faaa0f87543fa4a0de64dcc48b9c9308ee18f0f6ccdd3',
        balance: 30000,
    },
];
=======
import { useCallback } from 'react';
import toast from 'react-hot-toast';
import { useNavigate } from 'react-router-dom';

import { SummaryCard } from '../SummaryCard';
import Loading from '../loading';
import { useNextMenuUrl } from '../menu/hooks';
import Overlay from '../overlay';
import { type LedgerAccount } from './LedgerAccountItem';
import { SelectLedgerAccountsList } from './SelectLedgerAccountsList';
import { useDeriveLedgerAccounts } from './useDeriveLedgerAccounts';
import { Button } from '_src/ui/app/shared/ButtonUI';
import { Link } from '_src/ui/app/shared/Link';
import { Text } from '_src/ui/app/shared/text';

const numLedgerAccountsToDeriveByDefault = 10;
>>>>>>> 23ae97862 (work)

export function ImportLedgerAccounts() {
    const accountsUrl = useNextMenuUrl(true, `/accounts`);
    const navigate = useNavigate();
<<<<<<< HEAD
    const [suiLedgerClient] = useSuiLedgerClient();

    if (!suiLedgerClient) {
        // TODO (future improvement): We should detect when a user's Ledger device has disconnected so that
        // we can redirect them away from this route if they were to pull out their Ledger device mid-flow
        return <Navigate to={accountsUrl} replace />;
    }
=======

    const onDeriveError = useCallback(() => {
        navigate(accountsUrl, { replace: true });
        toast.error('There was an issue importing your accounts.');
    }, [accountsUrl, navigate]);

    const [ledgerAccounts, setLedgerAccounts, areLedgerAccountsLoading] =
        useDeriveLedgerAccounts({
            numAccountsToDerive: numLedgerAccountsToDeriveByDefault,
            onError: onDeriveError,
        });

    const onSelectAccount = useCallback(
        (selectedAccount: LedgerAccount) => {
            setLedgerAccounts((prevState) => {
                return prevState.map((account) => {
                    if (account.address === selectedAccount.address) {
                        return {
                            isSelected: !selectedAccount.isSelected,
                            address: selectedAccount.address,
                        };
                    }
                    return account;
                });
            });
        },
        [setLedgerAccounts]
    );

    const onSelectAllAccounts = useCallback(() => {
        setLedgerAccounts((prevState) => {
            return prevState.map((account) => {
                return {
                    isSelected: true,
                    address: account.address,
                };
            });
        });
    }, [setLedgerAccounts]);

    // TODO: Add logic to filter out already imported Ledger accounts
    // so we don't allow users to import the same account twice
    const filteredLedgerAccounts = ledgerAccounts.filter(() => true);
    const selectedLedgerAccounts = filteredLedgerAccounts.filter(
        (account) => account.isSelected
    );

    const numAccounts = ledgerAccounts.length;
    const numFilteredAccounts = filteredLedgerAccounts.length;
    const numSelectedAccounts = selectedLedgerAccounts.length;
    const areAllAccountsImported = numAccounts > 0 && numFilteredAccounts === 0;
    const areNoAccountsSelected = numSelectedAccounts === 0;
    const areAllAccountsSelected = numSelectedAccounts === numAccounts;
    const isSelectAllButtonDisabled =
        areAllAccountsSelected || areAllAccountsImported;
>>>>>>> 23ae97862 (work)

    return (
        <Overlay
            showModal
            title="Import Accounts"
            closeOverlay={() => {
                navigate(accountsUrl);
            }}
        >
            <div className="w-full flex flex-col">
                <SummaryCard
                    minimalPadding
                    header="Connect Ledger Accounts"
                    body={
<<<<<<< HEAD
                        <ul className="list-none h-[272px] m-0 p-0 -mr-2 mt-1 py-0 pr-2 overflow-auto custom-scrollbar">
                            {mockAccounts.map((account) => {
                                return (
                                    <li
                                        className="pt-2 pb-2 first:pt-1"
                                        key={account.address}
                                    >
                                        <LedgerAccount
                                            isSelected={account.isSelected}
                                            address={account.address}
                                            balance={account.balance}
                                        />
                                    </li>
                                );
                            })}
                        </ul>
=======
                        <Loading loading={areLedgerAccountsLoading}>
                            {/* TODO: This is just placeholder UI until we have finalized designs */}
                            {areAllAccountsImported ? (
                                <Text>All accounts are already imported</Text>
                            ) : (
                                <SelectLedgerAccountsList
                                    accounts={filteredLedgerAccounts}
                                    onSelect={onSelectAccount}
                                />
                            )}
                        </Loading>
>>>>>>> 23ae97862 (work)
                    }
                    footer={
                        <div className="rounded-b-2xl text-center">
                            <Link
                                text="Select All Accounts"
                                color="heroDark"
                                weight="medium"
<<<<<<< HEAD
=======
                                onClick={onSelectAllAccounts}
                                disabled={isSelectAllButtonDisabled}
>>>>>>> 23ae97862 (work)
                            />
                        </div>
                    }
                />
                <div className="mt-5">
                    <Button
                        variant="primary"
                        before={<UnlockedLockIcon />}
                        text="Unlock"
                        onClick={() => {
                            // TODO: Do work to actually import the selected accounts once we have
                            // the account infrastructure setup to support Ledger accounts
                            navigate(accountsUrl);
                        }}
<<<<<<< HEAD
=======
                        disabled={areNoAccountsSelected}
>>>>>>> 23ae97862 (work)
                    />
                </div>
            </div>
        </Overlay>
    );
}
