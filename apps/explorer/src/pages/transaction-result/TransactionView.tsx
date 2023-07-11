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
import { ErrorBoundary } from '~/components/error-boundary/ErrorBoundary';
import { useBreakpoint } from '~/hooks/useBreakpoint';
import { Events } from '~/pages/transaction-result/Events';
import { TransactionData } from '~/pages/transaction-result/TransactionData';
import { TransactionSummary } from '~/pages/transaction-result/transaction-summary';
import { Banner } from '~/ui/Banner';
import { PageHeader } from '~/ui/PageHeader';
import { SplitPanes } from '~/ui/SplitPanes';
import { Tabs, TabsContent, TabsList, TabsTrigger } from '~/ui/Tabs';

import styles from './TransactionResult.module.css';

export function TransactionView({ transaction }: { transaction: SuiTransactionBlockResponse }) {
	const isMediumOrAbove = useBreakpoint('md');

	const hasEvents = !!transaction.events?.length;

	const txError = getExecutionStatusError(transaction);

	const transactionKindName = getTransactionKindName(getTransactionKind(transaction)!);

	const isProgrammableTransaction = transactionKindName === 'ProgrammableTransaction';

	const leftPane = {
		panel: (
			<div className="h-full overflow-y-auto rounded-2xl border border-transparent bg-gray-40 p-6 md:h-full md:max-h-screen md:p-10">
				<Tabs size="lg" defaultValue="summary">
					<TabsList>
						<TabsTrigger value="summary">Summary</TabsTrigger>
						{hasEvents && <TabsTrigger value="events">Events</TabsTrigger>}
						{isProgrammableTransaction && <TabsTrigger value="signatures">Signatures</TabsTrigger>}
					</TabsList>
					<TabsContent value="summary">
						<div className="mt-10">
							<TransactionSummary transaction={transaction} />
						</div>
					</TabsContent>
					{hasEvents && (
						<TabsContent value="events">
							<div className="mt-10">
								<Events events={transaction.events!} />
							</div>
						</TabsContent>
					)}
					<TabsContent value="signatures">
						<div className="mt-10">
							<ErrorBoundary>
								<Signatures transaction={transaction} />
							</ErrorBoundary>
						</div>
					</TabsContent>
				</Tabs>
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
