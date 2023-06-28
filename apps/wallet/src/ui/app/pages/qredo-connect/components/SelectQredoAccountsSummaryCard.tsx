// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import { QredoAccountsSelector } from './QredoAccountsSelector';
import { useFetchQredoAccounts } from '../hooks';
import { SummaryCard } from '_components/SummaryCard';
import Alert from '_components/alert';
import Loading from '_components/loading';
import { type Wallet } from '_src/shared/qredo-api';
import { Link } from '_src/ui/app/shared/Link';

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
