// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import { useRecognizedPackages } from '_src/ui/app/hooks/useRecognizedPackages';
import { useTransactionSummary } from '@mysten/core';
import { type SuiTransactionBlockResponse } from '@mysten/sui/client';

import { DateCard } from '../../shared/date-card';
import { TransactionSummary } from '../../shared/transaction-summary';
import { ExplorerLinkCard } from '../../shared/transaction-summary/cards/ExplorerLink';
import { GasSummary } from '../../shared/transaction-summary/cards/GasSummary';
import { StakeTxnCard } from './StakeTxnCard';
import { StatusIcon } from './StatusIcon';
import { UnStakeTxnCard } from './UnstakeTxnCard';

type ReceiptCardProps = {
	txn: SuiTransactionBlockResponse;
	activeAddress: string;
};

function TransactionStatus({ success, timestamp }: { success: boolean; timestamp?: string }) {
	return (
		<div className="flex flex-col gap-3 items-center justify-center mb-4">
			<StatusIcon status={success} />
			<span data-testid="transaction-status" className="sr-only">
				{success ? 'Transaction Success' : 'Transaction Failed'}
			</span>
			{timestamp && <DateCard timestamp={Number(timestamp)} size="md" />}
		</div>
	);
}

export function ReceiptCard({ txn, activeAddress }: ReceiptCardProps) {
	const { events } = txn;
	const recognizedPackagesList = useRecognizedPackages();
	const summary = useTransactionSummary({
		transaction: txn,
		currentAddress: activeAddress,
		recognizedPackagesList,
	});

	if (!summary) return null;

	const stakedTxn = events?.find(({ type }) => type === '0x3::validator::StakingRequestEvent');

	const unstakeTxn = events?.find(({ type }) => type === '0x3::validator::UnstakingRequestEvent');

	// todo: re-using the existing staking cards for now
	if (stakedTxn || unstakeTxn)
		return (
			<div className="block relative w-full h-full">
				<TransactionStatus
					success={summary?.status === 'success'}
					timestamp={txn.timestampMs ?? undefined}
				/>
				<section className="-mx-5 bg-sui/10 min-h-full">
					<div className="px-5 py-10">
						<div className="flex flex-col gap-4">
							{stakedTxn ? <StakeTxnCard event={stakedTxn} /> : null}
							{unstakeTxn ? <UnStakeTxnCard event={unstakeTxn} /> : null}
							<GasSummary gasSummary={summary?.gas} />
							<ExplorerLinkCard
								digest={summary?.digest}
								timestamp={summary?.timestamp ?? undefined}
							/>
						</div>
					</div>
				</section>
			</div>
		);

	return (
		<div className="block relative w-full h-full">
			<TransactionStatus
				success={summary.status === 'success'}
				timestamp={txn.timestampMs ?? undefined}
			/>
			<TransactionSummary showGasSummary summary={summary} />
		</div>
	);
}
