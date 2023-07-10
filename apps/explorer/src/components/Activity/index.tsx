// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import { Filter16 } from '@mysten/icons';
import { useEffect, useState, createContext, useContext, useCallback } from 'react';
import { toast } from 'react-hot-toast';

import { EpochsActivityTable } from './EpochsActivityTable';
import { TransactionsActivityTable } from './TransactionsActivityTable';
import { CheckpointsTable } from '../checkpoints/CheckpointsTable';
import { genTableDataFromTxData } from '~/components/transactions/TxCardUtils';
import { useNetwork } from '~/context';
import { useGetTransactionBlocks } from '~/hooks/useGetTransactionBlocks';
import { DropdownMenu, DropdownMenuCheckboxItem } from '~/ui/DropdownMenu';
import { useCursorPagination } from '~/ui/Pagination';
import { PlayPause } from '~/ui/PlayPause';
import { Tabs, TabsContent, TabsList, TabsTrigger } from '~/ui/Tabs';
import { Network } from '~/utils/api/DefaultRpcClient';

const VALID_TABS = ['transactions', 'epochs', 'checkpoints'];

type Props = {
	initialTab?: string | null;
	initialLimit: number;
	disablePagination?: boolean;
};

type ActivityContextType = {
	transactionTable: ReturnType<typeof useTransactionActivityTable>;
	main: {
		activeTab: string;
		setActiveTab: (tab: string) => void;
		paused: boolean;
		setPaused: (paused: boolean) => void;
	};
};

export const ActivityContext = createContext<ActivityContextType | null>(null);

const AUTO_REFRESH_ID = 'auto-refresh';
const REFETCH_INTERVAL_SECONDS = 10;
const REFETCH_INTERVAL = REFETCH_INTERVAL_SECONDS * 1000;

function ActivityComponent({ initialTab, initialLimit, disablePagination }: Props) {
	const activityContext = useContext(ActivityContext);

	if (!activityContext) {
		throw new Error('ActivityComponent must be used within ActivityContext.Provider');
	}

	const { paused, setPaused, activeTab, setActiveTab } = activityContext.main;

	const handlePauseChange = () => {
		if (paused) {
			toast.success(`Auto-refreshing on - every ${REFETCH_INTERVAL_SECONDS} seconds`, {
				id: AUTO_REFRESH_ID,
			});
		} else {
			toast.success('Auto-refresh paused', { id: AUTO_REFRESH_ID });
		}

		setPaused(!paused);
	};

	const refetchInterval = paused ? undefined : REFETCH_INTERVAL;
	// TODO remove network check when querying transactions with TransactionKind filter is fixed on devnet and testnet
	const [network] = useNetwork();
	const isTransactionKindFilterEnabled = Network.MAINNET === network || Network.LOCAL === network;
	const [showSystemTransactions, setShowSystemTransaction] = useState(
		!isTransactionKindFilterEnabled,
	);
	useEffect(() => {
		if (!isTransactionKindFilterEnabled) {
			setShowSystemTransaction(true);
		}
	}, [isTransactionKindFilterEnabled]);

	return (
		<div>
			<Tabs size="lg" value={activeTab} onValueChange={setActiveTab}>
				<div className="relative">
					<TabsList>
						<TabsTrigger value="transactions">Transaction Blocks</TabsTrigger>
						<TabsTrigger value="epochs">Epochs</TabsTrigger>
						<TabsTrigger value="checkpoints">Checkpoints</TabsTrigger>
					</TabsList>
					<div className="absolute inset-y-0 -top-1 right-0 flex items-center gap-3 text-2xl">
						{activeTab === 'transactions' && isTransactionKindFilterEnabled ? (
							<DropdownMenu
								trigger={<Filter16 className="p-1" />}
								content={
									<DropdownMenuCheckboxItem
										checked={showSystemTransactions}
										label="Show System Transactions"
										onSelect={(e) => {
											e.preventDefault();
										}}
										onCheckedChange={() => {
											setShowSystemTransaction((value) => !value);
										}}
									/>
								}
								modal={false}
								align="end"
							/>
						) : null}

						{activeTab === 'transactions' && (
							<PlayPause
								paused={paused}
								onChange={handlePauseChange}
								animateDuration={REFETCH_INTERVAL}
							/>
						)}
					</div>
				</div>
				<TabsContent value="transactions">
					<TransactionsActivityTable
						refetchInterval={refetchInterval}
						initialLimit={initialLimit}
						disablePagination={disablePagination}
						paused={paused}
						transactionKindFilter={showSystemTransactions ? undefined : 'ProgrammableTransaction'}
					/>
				</TabsContent>
				<TabsContent value="epochs">
					<EpochsActivityTable initialLimit={initialLimit} disablePagination={disablePagination} />
				</TabsContent>
				<TabsContent value="checkpoints">
					<CheckpointsTable initialLimit={initialLimit} disablePagination={disablePagination} />
				</TabsContent>
			</Tabs>
		</div>
	);
}

const DEFAULT_TRANSACTIONS_LIMIT = 20;

function useTransactionActivityTable({
	initialLimit = DEFAULT_TRANSACTIONS_LIMIT,
	transactionKindFilter,
	paused,
	disabled,
}: {
	initialLimit?: number;
	transactionKindFilter?: 'ProgrammableTransaction';
	paused?: boolean;
	disabled?: boolean;
}) {
	const [limit, setLimit] = useState(initialLimit);
	const [startAnimationTimestamp, setStartAnimationTimestamp] = useState<number | undefined>();
	const transactions = useGetTransactionBlocks(
		transactionKindFilter ? { TransactionKind: transactionKindFilter } : undefined,
		limit,
		disabled,
	);
	const refetchInterval = paused ? undefined : REFETCH_INTERVAL;

	const { data, refetch, ...rest } = useCursorPagination(transactions);

	const handleRefetch = useCallback(async () => {
		await refetch();
		setStartAnimationTimestamp(performance.now());
	}, [refetch]);

	useEffect(() => {
		if (!paused && !disabled) {
			handleRefetch();
		}

		let timer: NodeJS.Timer;

		if (refetchInterval && !disabled) {
			timer = setInterval(() => {
				handleRefetch();
			}, refetchInterval);
		}

		return () => clearInterval(timer);
	}, [disabled, handleRefetch, paused, refetch, refetchInterval]);

	const cardData = data
		? genTableDataFromTxData(data.data, {
				renderAsTimestamp: paused,
		  })
		: undefined;

	return {
		...rest,
		cardData,
		limit,
		setLimit,
		startAnimationTimestamp,
	};
}

export function Activity(props: Props) {
	const [paused, setPaused] = useState(false);
	const { initialTab, initialLimit, disablePagination } = props;
	const [activeTab, setActiveTab] = useState(() =>
		initialTab && VALID_TABS.includes(initialTab) ? initialTab : 'transactions',
	);

	const transactionTable = useTransactionActivityTable({
		paused,
		initialLimit,
		transactionKindFilter: disablePagination ? undefined : 'ProgrammableTransaction',
		disabled: paused || activeTab !== 'transactions',
	});

	return (
		<ActivityContext.Provider
			value={{
				transactionTable,
				main: {
					activeTab,
					setActiveTab,
					paused,
					setPaused,
				},
			}}
		>
			<ActivityComponent {...props} />
		</ActivityContext.Provider>
	);
}
