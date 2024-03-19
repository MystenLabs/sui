// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import { cx } from 'class-variance-authority';
import { useMemo } from 'react';

import { useAccounts } from '../hooks/useAccounts';
import { Link } from '../shared/Link';
import { SummaryCard } from './SummaryCard';
import { WalletListSelectItem, type WalletListSelectItemProps } from './WalletListSelectItem';

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
	const { data: accounts } = useAccounts();
	const filteredAccounts = useMemo(() => {
		if (!accounts) {
			return [];
		}
		if (visibleValues) {
			return accounts.filter(({ address }) => visibleValues.includes(address));
		}
		return accounts;
	}, [accounts, visibleValues]);
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
					</div>
				) : null
			}
			minimalPadding
			boxShadow={boxShadow}
		/>
	);
}
