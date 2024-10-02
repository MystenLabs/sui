// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import { useTransactionData } from '_src/ui/app/hooks';
import { Tab as HeadlessTab, type TabProps } from '@headlessui/react';
import { type Transaction } from '@mysten/sui/transactions';

import { SummaryCard } from '../SummaryCard';
import { Command } from './Command';
import { Input } from './Input';

interface Props {
	sender?: string;
	transaction: Transaction;
}

const Tab = (props: TabProps<'div'>) => (
	<HeadlessTab
		className="border-0 border-b border-transparent ui-selected:border-hero text-steel-darker p-0 pb-2 -mb-px border-solid ui-selected:text-hero-dark text-body font-semibold bg-transparent outline-none cursor-pointer"
		{...props}
	/>
);

export function TransactionDetails({ sender, transaction }: Props) {
	const { data: transactionData, isPending, isError } = useTransactionData(sender, transaction);
	if (transactionData?.commands.length === 0 && transactionData.inputs.length === 0) {
		return null;
	}
	return (
		<SummaryCard header="Transaction Details" initialExpanded>
			{isPending || isError ? (
				<div className="ml-0 text-steel-darker text-pBodySmall font-medium">
					{isPending ? 'Gathering data...' : "Couldn't gather data"}
				</div>
			) : transactionData ? (
				<div>
					<HeadlessTab.Group>
						<HeadlessTab.List className="flex gap-6 border-0 border-b border-solid border-gray-45 mb-6">
							{!!transactionData.commands.length && <Tab>Commands</Tab>}
							{!!transactionData.inputs.length && <Tab>Inputs</Tab>}
						</HeadlessTab.List>
						<HeadlessTab.Panels>
							{!!transactionData.commands.length && (
								<HeadlessTab.Panel className="flex flex-col gap-6">
									{/* TODO: Rename components: */}
									{transactionData.commands.map((command, index) => (
										<Command key={index} command={command} />
									))}
								</HeadlessTab.Panel>
							)}
							{!!transactionData.inputs.length && (
								<HeadlessTab.Panel className="flex flex-col gap-2">
									{transactionData.inputs.map((input, index) => (
										<Input key={index} input={input} />
									))}
								</HeadlessTab.Panel>
							)}
						</HeadlessTab.Panels>
					</HeadlessTab.Group>
				</div>
			) : (
				''
			)}
		</SummaryCard>
	);
}
