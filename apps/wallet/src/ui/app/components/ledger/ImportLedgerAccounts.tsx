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

import { useNextMenuUrl } from '../menu/hooks';
import Overlay from '../overlay';
import { type LedgerAccount } from './LedgerAccountItem';
import { LedgerAccountList } from './LedgerAccountList';
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
        toast.error('Make sure you have the Sui application open.');
    }, [accountsUrl, navigate]);

    const [ledgerAccounts, setLedgerAccounts, areLedgerAccountsLoading] =
        useDeriveLedgerAccounts({
            numAccountsToDerive: numLedgerAccountsToDeriveByDefault,
            onError: onDeriveError,
        });

    const onAccountClick = useCallback(
        (targetAccount: LedgerAccount) => {
            setLedgerAccounts((prevState) =>
                prevState.map((account) => {
                    if (account.address === targetAccount.address) {
                        return {
                            isSelected: !targetAccount.isSelected,
                            address: targetAccount.address,
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
                isSelected: true,
                address: account.address,
            }))
        );
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
                    onClick={() => {
                        // TODO: Do work to actually import the selected accounts once we have
                        // the account infrastructure setup to support Ledger accounts
                        navigate(accountsUrl);
                    }}
                    disabled={areNoAccountsSelected}
                />
            </div>
        </Overlay>
    );
}
