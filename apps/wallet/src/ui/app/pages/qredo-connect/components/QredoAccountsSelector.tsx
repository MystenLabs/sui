// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import { Search16 } from '@mysten/icons';
import { useState } from 'react';

import { QredoAccountItem } from './QredoAccountItem';
import { type Wallet } from '_src/shared/qredo-api';

function matchesSearchTerm({ walletID, address, labels }: Wallet, searchTerm: string) {
	const term = searchTerm.trim().toLowerCase();
	return (
		!term ||
		walletID.toLowerCase().includes(term) ||
		address.toLowerCase().includes(term) ||
		labels.some(({ value }) => value.toLowerCase().includes(term))
	);
}

function isAccountSelected(selectedAccounts: Wallet[], account: Wallet) {
	return selectedAccounts.some(({ walletID }) => account.walletID === walletID);
}

export type QredoAccountsSelectorProps = {
	accounts: Wallet[];
	selectedAccounts: Wallet[];
	onChange: (selectedAccounts: Wallet[]) => void;
};

export function QredoAccountsSelector({
	accounts,
	selectedAccounts,
	onChange,
}: QredoAccountsSelectorProps) {
	const [searchTerm, setSearchTerm] = useState('');
	return (
		<>
			<div className="flex items-center bg-white pt-1 sticky -top-4">
				<input
					className="flex-1 p-3 pr-7.5 bg-white border border-solid border-gray-45 rounded-lg text-steel-dark placeholder:text-steel"
					onChange={(e) => setSearchTerm(e.target.value)}
					value={searchTerm}
					placeholder="Search"
				/>
				<Search16 className="absolute w-4.5 h-4.5 right-3 pointer-events-none text-steel" />
			</div>
			<div className="divide-x-0 divide-y divide-gray-40 divide-solid">
				{accounts
					.filter((aWallet) => matchesSearchTerm(aWallet, searchTerm))
					.map((aWallet) => (
						<QredoAccountItem
							key={aWallet.walletID}
							{...aWallet}
							selected={isAccountSelected(selectedAccounts, aWallet)}
							onClick={() => {
								const isSelected = isAccountSelected(selectedAccounts, aWallet);
								let newAccounts = [...selectedAccounts];
								if (isSelected) {
									newAccounts = newAccounts.filter(({ walletID }) => aWallet.walletID !== walletID);
								} else {
									newAccounts.push(aWallet);
								}
								onChange(newAccounts);
							}}
						/>
					))}
			</div>
		</>
	);
}
