// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import { useEffect, useMemo, useRef } from 'react';
import { QredoAccountsSelector } from './QredoAccountsSelector';
import { useFetchQredoAccounts } from '../hooks';
import { SummaryCard } from '_components/SummaryCard';
import Alert from '_components/alert';
import Loading from '_components/loading';
import { isQredoAccountSerializedUI } from '_src/background/accounts/QredoAccount';
import { isSerializedQredoAccount } from '_src/background/keyring/Account';
import { NEW_ACCOUNTS_ENABLED } from '_src/shared/constants';
import { type Wallet } from '_src/shared/qredo-api';
import { useAccounts as useAccountsV2 } from '_src/ui/app/hooks/accounts-v2/useAccounts';
import { useAccounts } from '_src/ui/app/hooks/useAccounts';
import { Link } from '_src/ui/app/shared/Link';

function useQredoConnectedAccounts() {
	const accounts = useAccounts();
	const qredoConnectedAccounts = useMemo(
		() => accounts.filter(isSerializedQredoAccount),
		[accounts],
	);
	const { data: accountsV2, isLoading, error } = useAccountsV2();
	const qredoConnectedAccountsV2 = useMemo(
		() => (isLoading || error || !accountsV2 ? [] : accountsV2.filter(isQredoAccountSerializedUI)),
		[accountsV2, isLoading, error],
	);
	if (NEW_ACCOUNTS_ENABLED) {
		return { accounts: qredoConnectedAccountsV2, isLoading, error };
	}
	return {
		accounts: qredoConnectedAccounts,
		isLoading: false,
		error: null,
	};
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
	const { data, isLoading, error } = useFetchQredoAccounts(qredoID, fetchAccountsEnabled);
	const { accounts: qredoConnectedAccounts } = useQredoConnectedAccounts();
	const selectedAccountRef = useRef(selectedAccounts);
	selectedAccountRef.current = selectedAccounts;
	useEffect(() => {
		if (qredoConnectedAccounts?.length && data?.length) {
			const newSelected = [...selectedAccountRef.current];
			data
				.filter(({ walletID }) => {
					for (const aConnectedAccount of qredoConnectedAccounts) {
						if (aConnectedAccount.walletID === walletID) {
							return true;
						}
					}
					return false;
				})
				.forEach((aConnectedWallet) => {
					if (
						!selectedAccountRef.current.find(
							({ walletID }) => walletID === aConnectedWallet.walletID,
						)
					) {
						newSelected.push(aConnectedWallet);
					}
				});
			if (newSelected.length !== selectedAccountRef.current.length) {
				onChange(newSelected);
			}
		}
	}, [qredoConnectedAccounts, data, onChange]);
	return (
		<SummaryCard
			header="Select accounts"
			body={
				<Loading loading={isLoading}>
					{error ? (
						<Alert>Failed to fetch accounts. Please try again later.</Alert>
					) : data?.length ? (
						<QredoAccountsSelector
							accounts={data}
							selectedAccounts={selectedAccounts}
							onChange={onChange}
						/>
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
