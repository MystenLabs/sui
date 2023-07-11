// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import { useTransactionSummary } from '@mysten/core';
import {
	getTransactionKind,
	getTransactionKindName,
	type ProgrammableTransaction,
	type SuiTransactionBlockResponse,
} from '@mysten/sui.js';

import { TransactionDetailCard } from './transaction-summary/TransactionDetailCard';
import { GasBreakdown } from '~/components/GasBreakdown';
import { InputsCard } from '~/pages/transaction-result/programmable-transaction-view/InputsCard';
import { TransactionsCard } from '~/pages/transaction-result/programmable-transaction-view/TransactionsCard';

interface Props {
	transaction: SuiTransactionBlockResponse;
}

export function TransactionData({ transaction }: Props) {
	const summary = useTransactionSummary({
		transaction,
	});

	const transactionKindName = getTransactionKindName(getTransactionKind(transaction)!);

	const isProgrammableTransaction = transactionKindName === 'ProgrammableTransaction';

	const programmableTxn = transaction.transaction!.data.transaction as ProgrammableTransaction;

	return (
		<div className="flex flex-wrap gap-6">
			<section className="flex w-96 flex-1 flex-col gap-6 max-md:min-w-[50%]">
				<TransactionDetailCard
					timestamp={summary?.timestamp}
					sender={summary?.sender}
					checkpoint={transaction.checkpoint}
					executedEpoch={transaction.effects?.executedEpoch}
				/>

				{isProgrammableTransaction && (
					<div data-testid="inputs-card">
						<InputsCard inputs={programmableTxn.inputs} />
					</div>
				)}
			</section>

			<section className="flex w-96 flex-1 flex-col gap-6 md:min-w-transactionColumn">
				{isProgrammableTransaction && (
					<>
						<div data-testid="transactions-card">
							<TransactionsCard transactions={programmableTxn.transactions} />
						</div>
						<div data-testid="gas-breakdown">
							<GasBreakdown summary={summary} />
						</div>
					</>
				)}
			</section>
		</div>
	);
}
