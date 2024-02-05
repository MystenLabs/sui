// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import { DryRunTransactionBlockResponse } from '@mysten/sui.js/src/client';
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
		},
		{
			name: 'object-changes',
			title: 'Object Changes',
			count: objectChanges?.length,
		},
		{
			name: 'events',
			title: 'Events',
			count: output.events.length,
		},
		{
			name: 'transactions',
			title: 'Transactions',
			count:
				output.input.transaction.kind === 'ProgrammableTransaction'
					? output.input.transaction.transactions.length
					: 0,
		},
		{
			name: 'json',
			title: 'Raw JSON',
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

				<Tabs.Content className="py-6 " value="balance-changes">
					<BalanceChanges changes={balanceChanges} />
				</Tabs.Content>

				<Tabs.Content className="py-6" value="object-changes">
					<ObjectChanges objects={objectChanges} />
				</Tabs.Content>

				<Tabs.Content className="py-6" value="events">
					<Events events={output.events} />
				</Tabs.Content>

				<Tabs.Content className="py-6" value="transactions">
					<Transactions inputs={output.input} />
				</Tabs.Content>

				<Tabs.Content className="py-6" value="json">
					<Textarea value={JSON.stringify(output, null, 4)} rows={20} />
				</Tabs.Content>
			</Tabs.Root>
		</>
	);
}
