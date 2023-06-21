// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import {
	getExecutionStatusError,
	getExecutionStatusType,
	getTransactionDigest,
	getTransactionKind,
	getTransactionKindName,
	type SuiTransactionBlockResponse,
} from '@mysten/sui.js';
import clsx from 'clsx';

// import {
//     eventToDisplay,
//     getAddressesLinks,
// } from '../../components/events/eventDisplay';

import { Signatures } from './Signatures';

import styles from './TransactionResult.module.css';

import { useBreakpoint } from '~/hooks/useBreakpoint';
import { Events } from '~/pages/transaction-result/Events';
import { TransactionData } from '~/pages/transaction-result/TransactionData';
import { TransactionSummary } from '~/pages/transaction-result/transaction-summary';
import { Banner } from '~/ui/Banner';
import { PageHeader } from '~/ui/PageHeader';
import { SplitPanes } from '~/ui/SplitPanes';
import { Tab, TabGroup, TabList, TabPanel, TabPanels } from '~/ui/Tabs';

export function TransactionView({ transaction }: { transaction: SuiTransactionBlockResponse }) {
	const isMediumOrAbove = useBreakpoint('md');

	const hasEvents = !!transaction.events?.length;

	const txError = getExecutionStatusError(transaction);

	const transactionKindName = getTransactionKindName(getTransactionKind(transaction)!);

	const isProgrammableTransaction = transactionKindName === 'ProgrammableTransaction';

	const leftPane = {
		panel: (
			<div className="h-full overflow-y-auto rounded-2xl border border-transparent bg-gray-40 p-6 md:h-full md:max-h-screen md:p-10">
				<TabGroup size="lg">
					<TabList>
						<Tab>Summary</Tab>
						{hasEvents && <Tab>Events</Tab>}
						{isProgrammableTransaction && <Tab>Signatures</Tab>}
					</TabList>
					<TabPanels>
						<TabPanel>
							<div className="mt-10">
								<TransactionSummary transaction={transaction} />
							</div>
						</TabPanel>
						{hasEvents && (
							<TabPanel>
								<div className="mt-10">
									<Events events={transaction.events!} />
								</div>
							</TabPanel>
						)}
						<TabPanel>
							<div className="mt-10">
								<Signatures transaction={transaction} />
							</div>
						</TabPanel>
					</TabPanels>
				</TabGroup>
			</div>
		),
		minSize: 35,
		collapsible: true,
		collapsibleButton: true,
		noHoverHidden: isMediumOrAbove,
	};

	const rightPane = {
		panel: (
			<div className="h-full w-full overflow-y-auto md:overflow-y-hidden">
				<TransactionData transaction={transaction} />
			</div>
		),
		minSize: 40,
		defaultSize: isProgrammableTransaction ? 65 : 50,
	};

	return (
		<div className={clsx(styles.txdetailsbg)}>
			<div className="mb-10">
				<PageHeader
					type="Transaction"
					title={getTransactionDigest(transaction)}
					subtitle={!isProgrammableTransaction ? transactionKindName : undefined}
					status={getExecutionStatusType(transaction)}
				/>
				{txError && (
					<div className="mt-2">
						<Banner variant="error">{txError}</Banner>
					</div>
				)}
			</div>
			<div className="h-screen md:h-full">
				<SplitPanes
					splitPanels={[leftPane, rightPane]}
					direction={isMediumOrAbove ? 'horizontal' : 'vertical'}
				/>
			</div>
		</div>
	);
}
