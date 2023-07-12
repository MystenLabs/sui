// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import { type Meta, type StoryObj } from '@storybook/react';
import { useState, useMemo } from 'react';

import { Search, type SearchProps } from '../Search';

export default {
	component: Search,
} as Meta;

const options = [
	{
		label: 'transaction',
		results: [
			{ id: 1, label: 'transaction 1' },
			{ id: 2, label: 'transaction 2' },
			{ id: 3, label: 'transaction 3' },
			{ id: 4, label: 'transaction 4' },
		],
	},
	{
		label: 'object',
		results: [
			{ id: 1, label: 'object 1' },
			{ id: 2, label: 'object 2' },
			{ id: 3, label: 'object 3' },
			{ id: 4, label: 'object 4' },
		],
	},
	{
		label: 'address',
		results: [
			{ id: 1, label: 'address 1' },
			{ id: 2, label: 'address 2' },
			{ id: 3, label: 'address 3' },
			{ id: 4, label: 'address 4' },
		],
	},
];

export const Default: StoryObj<SearchProps> = {
	args: {},
	render: () => {
		const [query, setQuery] = useState('');
		const filteredOptions = useMemo(() => {
			const filtered = options.reduce((acc, curr) => {
				const filtered = curr.results.filter((option) =>
					option.label.toLowerCase().includes(query.toLowerCase()),
				);
				if (filtered.length) {
					acc.push({ label: curr.label, results: filtered });
				}
				return acc;
			}, [] as any);
			return filtered;
		}, [query]);

		return (
			<div className="flex h-screen w-screen bg-headerNav p-10">
				<div className="flex max-w-xl flex-1">
					<Search
						queryValue={query}
						isLoading={false}
						onChange={(value) => setQuery(value)}
						onSelectResult={(result) => setQuery(result.label)}
						placeholder="Search Addresses / Objects / Transactions"
						options={filteredOptions}
					/>
				</div>
			</div>
		);
	},
};
