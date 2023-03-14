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

import { useAccounts } from '../../hooks/useAccounts';
import { useNextMenuUrl } from '../menu/hooks';
import Overlay from '../overlay';
import { LedgerAccountList } from './LedgerAccountList';
import {
    type SelectableLedgerAccount,
    useDeriveLedgerAccounts,
} from './useDeriveLedgerAccounts';
import { useImportLedgerAccountsMutation } from './useImportLedgerAccountsMutation';
import { Button } from '_src/ui/app/shared/ButtonUI';
import { Link } from '_src/ui/app/shared/Link';
import { Text } from '_src/ui/app/shared/text';

const numLedgerAccountsToDeriveByDefault = 10;

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

    const importLedgerAccountsMutation = useImportLedgerAccountsMutation({
        onSuccess: () => navigate(accountsUrl),
        onError: () => {
            toast.error('There was an issue importing your Ledger accounts.');
        },
    });

    const existingAccounts = useAccounts();
    const existingAccountAddresses = existingAccounts.map(
        (account) => account.address
    );
    const filteredLedgerAccounts = ledgerAccounts.filter(
        (account) => !existingAccountAddresses.includes(account.address)
    );
    const selectedLedgerAccounts = filteredLedgerAccounts.filter(
        (account) => account.isSelected
    );

    const onAccountClick = useCallback(
        (targetAccount: SelectableLedgerAccount) => {
            setLedgerAccounts((prevState) =>
                prevState.map((account) => {
                    if (account.address === targetAccount.address) {
                        return {
                            ...targetAccount,
                            isSelected: !targetAccount.isSelected,
                        };
                    }
                    return account;
                })
            );
        },
        [setLedgerAccounts]
    );

    const onSelectAllAccountsClick = useCallback(() => {
        setLedgerAccounts((prevState) =>
            prevState.map((account) => ({
                ...account,
                isSelected: true,
            }))
        );
    }, [setLedgerAccounts]);

    const numAccounts = ledgerAccounts.length;
    const numFilteredAccounts = filteredLedgerAccounts.length;
    const numSelectedAccounts = selectedLedgerAccounts.length;
    const areAllAccountsImported = numAccounts > 0 && numFilteredAccounts === 0;
    const areNoAccountsSelected = numSelectedAccounts === 0;
    const areAllAccountsSelected = numSelectedAccounts === numAccounts;
    const isSelectAllButtonDisabled =
        areAllAccountsSelected || areAllAccountsImported;

    let summaryCardBody: JSX.Element | null = null;
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
            <div className="max-h-[272px] -mr-2 mt-1 pr-2 overflow-auto custom-scrollbar">
                <LedgerAccountList
                    accounts={filteredLedgerAccounts}
                    onAccountClick={onAccountClick}
                />
            </div>
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
            <div className="w-full flex flex-col gap-5">
                <div className="h-full bg-white flex flex-col border border-solid border-gray-45 rounded-2xl">
                    <div className="text-center bg-gray-40 py-2.5 rounded-t-2xl">
                        <Text
                            variant="captionSmall"
                            weight="bold"
                            color="steel-darker"
                            truncate
                        >
                            {areAllAccountsImported
                                ? 'Ledger Accounts '
                                : 'Connect Ledger Accounts'}
                        </Text>
                    </div>
                    <div className="grow px-4 py-2">{summaryCardBody}</div>
                    <div className="w-full rounded-b-2xl border-x-0 border-b-0 border-t border-solid border-gray-40 text-center pt-3 pb-4">
                        <div className="w-fit ml-auto mr-auto">
                            <Link
                                text="Select All Accounts"
                                color="heroDark"
                                weight="medium"
                                onClick={onSelectAllAccountsClick}
                                disabled={isSelectAllButtonDisabled}
                            />
                        </div>
                    </div>
                </div>
                <Button
                    variant="primary"
                    before={<UnlockedLockIcon />}
                    text="Unlock"
                    loading={importLedgerAccountsMutation.isLoading}
                    onClick={() =>
                        importLedgerAccountsMutation.mutate(
                            selectedLedgerAccounts
                        )
                    }
                    disabled={areNoAccountsSelected}
                />
            </div>
        </Overlay>
    );
}
