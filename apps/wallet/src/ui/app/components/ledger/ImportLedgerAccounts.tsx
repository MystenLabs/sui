// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import { LockUnlocked16 as UnlockedLockIcon } from '@mysten/icons';
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

export function ImportLedgerAccounts() {
    const accountsUrl = useNextMenuUrl(true, `/accounts`);
    const navigate = useNavigate();

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
                    }
                    footer={
                        <div className="rounded-b-2xl text-center">
                            <Link
                                text="Select All Accounts"
                                color="heroDark"
                                weight="medium"
                                onClick={onSelectAllAccounts}
                                disabled={isSelectAllButtonDisabled}
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
                        disabled={areNoAccountsSelected}
                    />
                </div>
            </div>
        </Overlay>
    );
}
