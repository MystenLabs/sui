// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import { Button } from '_src/ui/app/shared/ButtonUI';
import { Link } from '_src/ui/app/shared/Link';
import { Text } from '_src/ui/app/shared/text';
import {
	Spinner16 as SpinnerIcon,
	ThumbUpStroke32 as ThumbUpIcon,
	LockUnlocked16 as UnlockedLockIcon,
} from '@mysten/icons';
import { useCallback, useEffect, useState } from 'react';
import toast from 'react-hot-toast';
import { useNavigate, useSearchParams } from 'react-router-dom';

import { useAccountsFormContext } from '../../components/accounts/AccountsFormContext';
import {
	LedgerAccountList,
	type SelectableLedgerAccount,
} from '../../components/ledger/LedgerAccountList';
import {
	useDeriveLedgerAccounts,
	type DerivedLedgerAccount,
} from '../../components/ledger/useDeriveLedgerAccounts';
import Overlay from '../../components/overlay';
import { getSuiApplicationErrorMessage } from '../../helpers/errorMessages';
import { useAccounts } from '../../hooks/useAccounts';

const numLedgerAccountsToDeriveByDefault = 10;

export function ImportLedgerAccountsPage() {
	const [searchParams] = useSearchParams();
	const successRedirect = searchParams.get('successRedirect') || '/tokens';
	const navigate = useNavigate();
	const { data: existingAccounts } = useAccounts();
	const [selectedLedgerAccounts, setSelectedLedgerAccounts] = useState<DerivedLedgerAccount[]>([]);
	const {
		data: ledgerAccounts,
		error: ledgerError,
		isPending: areLedgerAccountsLoading,
		isError: encounteredDerviceAccountsError,
	} = useDeriveLedgerAccounts({
		numAccountsToDerive: numLedgerAccountsToDeriveByDefault,
		select: (ledgerAccounts) => {
			return ledgerAccounts.filter(
				({ address }) => !existingAccounts?.some((account) => account.address === address),
			);
		},
	});

	useEffect(() => {
		if (ledgerError) {
			toast.error(getSuiApplicationErrorMessage(ledgerError) || 'Something went wrong.');
			navigate(-1);
		}
	}, [ledgerError, navigate]);

	const onAccountClick = useCallback(
		(targetAccount: SelectableLedgerAccount) => {
			if (targetAccount.isSelected) {
				setSelectedLedgerAccounts((prevState) =>
					prevState.filter((ledgerAccount) => {
						return ledgerAccount.address !== targetAccount.address;
					}),
				);
			} else {
				setSelectedLedgerAccounts((prevState) => [...prevState, targetAccount]);
			}
		},
		[setSelectedLedgerAccounts],
	);
	const numImportableAccounts = ledgerAccounts?.length;
	const numSelectedAccounts = selectedLedgerAccounts.length;
	const areAllAccountsImported = numImportableAccounts === 0;
	const areAllAccountsSelected = numSelectedAccounts === numImportableAccounts;
	const isUnlockButtonDisabled = numSelectedAccounts === 0;
	const isSelectAllButtonDisabled = areAllAccountsImported || areAllAccountsSelected;
	const [, setAccountsFormValues] = useAccountsFormContext();

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
		const selectedLedgerAddresses = selectedLedgerAccounts.map(({ address }) => address);
		summaryCardBody = (
			<div className="max-h-[272px] -mr-2 mt-1 pr-2 overflow-auto custom-scrollbar">
				<LedgerAccountList
					accounts={ledgerAccounts.map((ledgerAccount) => ({
						...ledgerAccount,
						isSelected: selectedLedgerAddresses.includes(ledgerAccount.address),
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
				navigate(-1);
			}}
		>
			<div className="w-full h-full flex flex-col gap-5">
				<div className="h-full max-h-[368px] bg-white flex flex-col border border-solid border-gray-45 rounded-2xl">
					<div className="text-center bg-gray-40 py-2.5 rounded-t-2xl">
						<Text variant="captionSmall" weight="bold" color="steel-darker" truncate>
							{areAllAccountsImported ? 'Ledger Accounts ' : 'Connect Ledger Accounts'}
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
										setSelectedLedgerAccounts(ledgerAccounts);
									}
								}}
								disabled={isSelectAllButtonDisabled}
							/>
						</div>
					</div>
				</div>
				<div className="flex items-end flex-1">
					<Button
						variant="primary"
						size="tall"
						before={<UnlockedLockIcon />}
						text="Next"
						disabled={isUnlockButtonDisabled}
						onClick={() => {
							setAccountsFormValues({
								type: 'ledger',
								accounts: selectedLedgerAccounts.map(({ address, derivationPath, publicKey }) => ({
									address,
									derivationPath,
									publicKey: publicKey!,
								})),
							});
							navigate(
								`/accounts/protect-account?${new URLSearchParams({
									accountType: 'ledger',
									successRedirect,
								}).toString()}`,
							);
						}}
					/>
				</div>
			</div>
		</Overlay>
	);
}
