// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import { useEffect, useState } from 'react';
import { darkStyles, JsonView } from 'react-json-view-lite';
import { useLocation } from 'react-router-dom';

import 'react-json-view-lite/dist/index.css';

import * as Tabs from '@radix-ui/react-tabs';

import { ReplayType } from '@/components/replay/replay-types';
import { ReplayOverview } from '@/components/replay/ReplayOverview';
import { ReplayTransactionBlocks } from '@/components/replay/ReplayTransactionBlocks';
import { objectToCamelCase } from '@/lib/utils';
import { ReplayLink } from '@/components/replay/ReplayLink';
import { ReplayEffects } from '@/components/replay/ReplayEffects';

// SPDX-License-Identifier: Apache-2.0
export function Replay() {
	const { hash } = useLocation();
	const [data, setData] = useState<ReplayType>();
	const [tab, setTab] = useState('overview');

	useEffect(() => {
		if (hash) {
			setData(objectToCamelCase(JSON.parse(decodeURIComponent(hash.slice(1)))) as ReplayType);
		}
	}, [hash]);

	const tabs = [
		{
			name: 'overview',
			title: 'Overview',
			component: () =>
				data && (
					<ReplayOverview
						effects={data.effects}
						gasStatus={data.gasStatus}
						inputs={data.transactionInfo.ProgrammableTransaction.inputs}
					/>
				),
		},
		{
			name: 'effects',
			title: 'Effects',
			component: () => <ReplayEffects />,
		},
		{
			name: 'transactions',
			title: 'PTB Commands',
			component: () =>
				data && (
					<ReplayTransactionBlocks transactions={data.transactionInfo.ProgrammableTransaction} />
				),
		},
		{
			name: 'json',
			title: 'Raw JSON',
			component: () => <JsonView data={data as object} style={darkStyles} />,
		},
	];

	return (
		<>
		<div className="mb-6">
			<h1 className="text-lg font-bold">Transaction Replay</h1>
			<ReplayLink digest={data?.effects.transactionDigest} text={data?.effects.transactionDigest!} network='mainnet' />
		</div>

		<Tabs.Root value={tab} onValueChange={setTab} className="w-full">
			<Tabs.List className="flex overflow-x-auto border-b ">
				{tabs.map((tab, index) => {
					return (
						<Tabs.Trigger
							key={index}
							className="border-transparent data-[state=active]:bg-gray-100 px-3 py-1 rounded-t-sm data-[state=active]:text-black"
							value={tab.name}
						>
							{tab.title}
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
