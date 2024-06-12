// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import { DryRunTransactionBlockResponse } from '@mysten/sui/client';
import * as Tabs from '@radix-ui/react-tabs';
import { useState } from 'react';

import { Textarea } from '../ui/textarea';
import { BalanceChanges } from './partials/BalanceChanges';
import { Events } from './partials/Events';
import { ObjectChanges } from './partials/ObjectChanges';
import { Overview } from './partials/Overview';
import { Transactions } from './partials/Transactions';

export function EffectsPreview({ output }: { output: DryRunTransactionBlockResponse }) {
	const [tab, setTab] = useState('balance-changes');

	const { objectChanges, balanceChanges } = output;

	const tabs = [
		{
			name: 'balance-changes',
			title: 'Balance Changes',
			count: balanceChanges?.length,
			component: () => <BalanceChanges changes={balanceChanges} />,
		},
		{
			name: 'object-changes',
			title: 'Object Changes',
			count: objectChanges?.length,
			component: () => <ObjectChanges objects={objectChanges} />,
		},
		{
			name: 'events',
			title: 'Events',
			count: output.events.length,
			component: () => <Events events={output.events} />,
		},
		{
			name: 'transactions',
			title: 'Transactions',
			count:
				output.input.transaction.kind === 'ProgrammableTransaction'
					? output.input.transaction.transactions.length
					: 0,
			component: () => <Transactions inputs={output.input} />,
		},
		{
			name: 'json',
			title: 'Raw JSON',
			component: () => <Textarea value={JSON.stringify(output, null, 4)} rows={20} />,
		},
	];

	return (
		<>
			<Overview output={output} />
			<Tabs.Root value={tab} onValueChange={setTab} className="w-full">
				<Tabs.List className="flex overflow-x-auto border-b ">
					{tabs.map((tab, index) => {
						return (
							<Tabs.Trigger
								key={index}
								className="border-transparent data-[state=active]:bg-gray-100 px-3 py-1 rounded-t-sm data-[state=active]:text-black"
								value={tab.name}
							>
								{tab.title} {!!tab.count && Number(tab.count) > 0 && `(${tab.count})`}
							</Tabs.Trigger>
						);
					})}
				</Tabs.List>

				{tabs.map((tab, index) => {
					return (
						<Tabs.Content key={index} value={tab.name} className="py-6">
							{tab.component()}
						</Tabs.Content>
					);
				})}
			</Tabs.Root>
		</>
	);
}
