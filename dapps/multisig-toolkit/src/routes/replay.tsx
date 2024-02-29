// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import { useEffect, useState } from 'react';
import { darkStyles, JsonView } from 'react-json-view-lite';
import { useLocation, useSearchParams } from 'react-router-dom';

import 'react-json-view-lite/dist/index.css';

import * as Tabs from '@radix-ui/react-tabs';

import { ReplayType } from '@/components/replay/replay-types';
import { ReplayContext } from '@/components/replay/ReplayContext';
import { ReplayEffects } from '@/components/replay/ReplayEffects';
import { ReplayLink } from '@/components/replay/ReplayLink';
import { ReplayOverview } from '@/components/replay/ReplayOverview';
import { ReplayTransactionBlocks } from '@/components/replay/ReplayTransactionBlocks';
import { objectToCamelCase } from '@/lib/utils';

export function Replay() {
	const { hash } = useLocation();

	const [data, setData] = useState<ReplayType>();
	const [searchParams] = useSearchParams();
	const [network, setNetwork] = useState('');

	const [tab, setTab] = useState('overview');

	useEffect(() => {
		if (hash) {
			setData(objectToCamelCase(JSON.parse(decodeURIComponent(hash.slice(1)))) as ReplayType);
		}
	}, [hash]);

	useEffect(() => {
		const urlNetwork = searchParams.get('network');
		if (urlNetwork && urlNetwork !== network) setNetwork(urlNetwork);
		// eslint-disable-next-line react-hooks/exhaustive-deps
	}, [searchParams]);

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
			component: () => data && <ReplayEffects mutated={data.effects.mutated} />,
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
		// The context allows for any child component to access the network and data
		// without explicitly passing it down as props.
		<ReplayContext.Provider
			value={{
				network,
				data: data ? data : null,
			}}
		>
			<div className="mb-6">
				<h1 className="text-lg font-bold">Transaction Replay</h1>
				<ReplayLink
					digest={data?.effects.transactionDigest}
					text={data?.effects.transactionDigest!}
				/>
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
		</ReplayContext.Provider>
	);
}
