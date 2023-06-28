// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import { cx } from 'class-variance-authority';
import { useMemo, useState } from 'react';

import { SummaryCard } from './SummaryCard';
import { WalletListSelectItem, type WalletListSelectItemProps } from './WalletListSelectItem';
import { useAccounts } from '../hooks/useAccounts';
import { useDeriveNextAccountMutation } from '../hooks/useDeriveNextAccountMutation';
import { Link } from '../shared/Link';

export type WalletListSelectProps = {
	title: string;
	values: string[];
	visibleValues?: string[];
	mode?: WalletListSelectItemProps['mode'];
	disabled?: boolean;
	onChange: (values: string[]) => void;
	boxShadow?: boolean;
};

export function WalletListSelect({
	title,
	values,
	visibleValues,
	mode = 'select',
	disabled = false,
	onChange,
	boxShadow = false,
}: WalletListSelectProps) {
	const [newAccounts, setNewAccounts] = useState<string[]>([]);
	const accounts = useAccounts();
	const filteredAccounts = useMemo(() => {
		if (visibleValues) {
			return accounts.filter(({ address }) => visibleValues.includes(address));
		}
		return accounts;
	}, [accounts, visibleValues]);
	const deriveNextAccount = useDeriveNextAccountMutation();
	return (
		<SummaryCard
			header={title}
			body={
				<ul
					className={cx(
						'flex flex-col items-stretch flex-1 p-0 m-0 self-stretch list-none',
						disabled ? 'opacity-70' : '',
					)}
				>
					{filteredAccounts.map(({ address }) => (
						<li
							key={address}
							onClick={() => {
								if (disabled) {
									return;
								}
								const newValues = [];
								let found = false;
								for (const anAddress of values) {
									if (anAddress === address) {
										found = true;
										continue;
									}
									newValues.push(anAddress);
								}
								if (!found) {
									newValues.push(address);
								}
								onChange(newValues);
							}}
						>
							<WalletListSelectItem
								address={address}
								selected={values.includes(address)}
								mode={mode}
								disabled={disabled}
								isNew={newAccounts.includes(address)}
							/>
						</li>
					))}
				</ul>
			}
			footer={
				mode === 'select' ? (
					<div className="flex flex-row flex-nowrap self-stretch justify-between">
						<div>
							{filteredAccounts.length > 1 ? (
								<Link
									color="heroDark"
									weight="medium"
									text="Select all"
									disabled={disabled}
									onClick={() => onChange(filteredAccounts.map(({ address }) => address))}
								/>
							) : null}
						</div>
						<div>
							<Link
								color="heroDark"
								weight="medium"
								text="New account"
								disabled={disabled}
								loading={deriveNextAccount.isLoading}
								onClick={async () => {
									const newAccountAddress = await deriveNextAccount.mutateAsync();
									setNewAccounts([...newAccounts, newAccountAddress]);
									if (!visibleValues || visibleValues.includes(newAccountAddress)) {
										onChange([...values, newAccountAddress]);
									}
								}}
							/>
						</div>
					</div>
				) : null
			}
			minimalPadding
			boxShadow={boxShadow}
		/>
	);
}
