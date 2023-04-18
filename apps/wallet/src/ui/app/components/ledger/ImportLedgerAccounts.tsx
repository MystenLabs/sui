// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import {
    LockUnlocked16 as UnlockedLockIcon,
    Spinner16 as SpinnerIcon,
    ThumbUpStroke32 as ThumbUpIcon,
} from '@mysten/icons';
import { useCallback, useState } from 'react';
import toast from 'react-hot-toast';
import { useNavigate } from 'react-router-dom';

import { getSuiApplicationErrorMessage } from '../../helpers/errorMessages';
import { useAccounts } from '../../hooks/useAccounts';
import { useNextMenuUrl } from '../menu/hooks';
import Overlay from '../overlay';
import {
    LedgerAccountList,
    type SelectableLedgerAccount,
} from './LedgerAccountList';
import { useDeriveLedgerAccounts } from './useDeriveLedgerAccounts';
import { useImportLedgerAccountsMutation } from './useImportLedgerAccountsMutation';
import { type SerializedLedgerAccount } from '_src/background/keyring/LedgerAccount';
import { Button } from '_src/ui/app/shared/ButtonUI';
import { Link } from '_src/ui/app/shared/Link';
import { Text } from '_src/ui/app/shared/text';

const numLedgerAccountsToDeriveByDefault = 10;

export function ImportLedgerAccounts() {
    const accountsUrl = useNextMenuUrl(true, `/accounts`);
    const navigate = useNavigate();

    const existingAccounts = useAccounts();
    const [selectedLedgerAccounts, setSelectedLedgerAccounts] = useState<
        SerializedLedgerAccount[]
    >([]);

    const {
        data: ledgerAccounts,
        isLoading: areLedgerAccountsLoading,
        isError: encounteredDerviceAccountsError,
    } = useDeriveLedgerAccounts({
        numAccountsToDerive: numLedgerAccountsToDeriveByDefault,
        select: (ledgerAccounts) => {
            return ledgerAccounts.filter(
                ({ address }) =>
                    !existingAccounts.some(
                        (account) => account.address === address
                    )
            );
        },
        onError: (error) => {
            toast.error(
                getSuiApplicationErrorMessage(error) || 'Something went wrong.'
            );
            navigate(accountsUrl, { replace: true });
        },
    });

    const importLedgerAccountsMutation = useImportLedgerAccountsMutation({
        onSuccess: () => navigate(accountsUrl),
        onError: () => {
            toast.error('There was an issue importing your Ledger accounts.');
        },
    });

    const onAccountClick = useCallback(
        (targetAccount: SelectableLedgerAccount) => {
            if (targetAccount.isSelected) {
                setSelectedLedgerAccounts((prevState) =>
                    prevState.filter((ledgerAccount) => {
                        return ledgerAccount.address !== targetAccount.address;
                    })
                );
            } else {
                setSelectedLedgerAccounts((prevState) => [
                    ...prevState,
                    targetAccount,
                ]);
            }
        },
        [setSelectedLedgerAccounts]
    );

    const numImportableAccounts = ledgerAccounts?.length;
    const numSelectedAccounts = selectedLedgerAccounts.length;

    const areAllAccountsImported = numImportableAccounts === 0;
    const areAllAccountsSelected =
        numSelectedAccounts === numImportableAccounts;

    const isUnlockButtonDisabled = numSelectedAccounts === 0;
    const isSelectAllButtonDisabled =
        areAllAccountsImported || areAllAccountsSelected;

    let summaryCardBody: JSX.Element | null = null;
    if (areLedgerAccountsLoading) {
        summaryCardBody = (
            <div className="w-full h-full flex flex-col justify-center items-center gap-2">
                <SpinnerIcon className="animate-spin text-steel w-4 h-4" />
                <Text variant="pBodySmall" color="steel-darker">
                    Looking for accounts
                </Text>
            </div>
        );
    } else if (areAllAccountsImported) {
        summaryCardBody = (
            <div className="w-full h-full flex flex-col justify-center items-center gap-2">
                <ThumbUpIcon className="text-steel w-8 h-8" />
                <Text variant="pBodySmall" color="steel-darker">
                    All Ledger accounts have been imported.
                </Text>
            </div>
        );
    } else if (!encounteredDerviceAccountsError) {
        const selectedLedgerAddresses = selectedLedgerAccounts.map(
            ({ address }) => address
        );
        summaryCardBody = (
            <div className="max-h-[272px] -mr-2 mt-1 pr-2 overflow-auto custom-scrollbar">
                <LedgerAccountList
                    accounts={ledgerAccounts.map((ledgerAccount) => ({
                        ...ledgerAccount,
                        isSelected: selectedLedgerAddresses.includes(
                            ledgerAccount.address
                        ),
                    }))}
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
                                onClick={() => {
                                    if (ledgerAccounts) {
                                        setSelectedLedgerAccounts(
                                            ledgerAccounts
                                        );
                                    }
                                }}
                                disabled={isSelectAllButtonDisabled}
                            />
                        </div>
                    </div>
                </div>
                <div>
                    <Button
                        variant="primary"
                        size="tall"
                        before={<UnlockedLockIcon />}
                        text="Unlock"
                        loading={importLedgerAccountsMutation.isLoading}
                        disabled={isUnlockButtonDisabled}
                        onClick={() =>
                            importLedgerAccountsMutation.mutate(
                                selectedLedgerAccounts
                            )
                        }
                    />
                </div>
            </div>
        </Overlay>
    );
}
