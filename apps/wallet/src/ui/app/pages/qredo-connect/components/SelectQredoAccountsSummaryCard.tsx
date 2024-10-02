// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import Alert from '_components/alert';
import Loading from '_components/loading';
import { SummaryCard } from '_components/SummaryCard';
import { isQredoAccountSerializedUI } from '_src/background/accounts/QredoAccount';
import { type Wallet } from '_src/shared/qredo-api';
import { useAccounts } from '_src/ui/app/hooks/useAccounts';
import { Link } from '_src/ui/app/shared/Link';
import { useEffect, useMemo, useRef } from 'react';

import { useFetchQredoAccounts } from '../hooks';
import { QredoAccountsSelector } from './QredoAccountsSelector';

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
	const { data, isPending, error } = useFetchQredoAccounts(qredoID, fetchAccountsEnabled);
	const { data: allAccounts } = useAccounts();
	const qredoConnectedAccounts = useMemo(
		() => allAccounts?.filter(isQredoAccountSerializedUI) || null,
		[allAccounts],
	);
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
			header="Select Qredo accounts"
			body={
				<Loading loading={isPending}>
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
