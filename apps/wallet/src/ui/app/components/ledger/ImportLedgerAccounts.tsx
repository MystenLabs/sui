// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import {
    LockUnlocked16 as UnlockedLockIcon,
    Spinner16 as SpinnerIcon,
    ThumbUpStroke32 as ThumbUpIcon,
} from '@mysten/icons';
import { useCallback } from 'react';
import toast from 'react-hot-toast';
import { useNavigate } from 'react-router-dom';

import { SummaryCard } from '../SummaryCard';
import { useNextMenuUrl } from '../menu/hooks';
import Overlay from '../overlay';
import { type LedgerAccount } from './LedgerAccountItem';
import { SelectLedgerAccountsList } from './SelectLedgerAccountsList';
import { useDeriveLedgerAccounts } from './useDeriveLedgerAccounts';
import { Button } from '_src/ui/app/shared/ButtonUI';
import { Link } from '_src/ui/app/shared/Link';
import { Text } from '_src/ui/app/shared/text';

const numLedgerAccountsToDeriveByDefault = 2;

export function ImportLedgerAccounts() {
    const accountsUrl = useNextMenuUrl(true, `/accounts`);
    const navigate = useNavigate();

    const onDeriveError = useCallback(() => {
        navigate(accountsUrl, { replace: true });
        toast.error('Make sure you have the Sui application open.');
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

    let summaryCardBody: JSX.Element | undefined;
    if (areLedgerAccountsLoading) {
        summaryCardBody = (
            <div className="w-full h-full flex flex-col justify-center items-center gap-3">
                <SpinnerIcon className="animate-spin text-steel w-4 h-4" />
                <Text variant="p2" color="steel-darker">
                    Looking for accounts
                </Text>
            </div>
        );
    } else if (areAllAccountsImported) {
        summaryCardBody = (
            <div className="w-full h-full flex flex-col justify-center items-center gap-2">
                <ThumbUpIcon className="text-steel w-8 h-8" />
                <Text variant="p2" color="steel-darker">
                    All Ledger accounts have been imported.
                </Text>
            </div>
        );
    } else {
        summaryCardBody = (
            <SelectLedgerAccountsList
                accounts={filteredLedgerAccounts}
                onSelect={onSelectAccount}
            />
        );
    }

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
                    header={
                        areAllAccountsImported
                            ? 'Ledger Accounts '
                            : 'Connect Ledger Accounts'
                    }
                    body={<div className="h-[272px]">{summaryCardBody}</div>}
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
