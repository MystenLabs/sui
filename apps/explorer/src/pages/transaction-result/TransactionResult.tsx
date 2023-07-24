// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import { getExecutionStatusError, type SuiTransactionBlockResponse } from '@mysten/sui.js';
import { LoadingIndicator } from '@mysten/ui';
import { useParams } from 'react-router-dom';

import { TransactionView } from './TransactionView';
import { PageLayout } from '~/components/Layout/PageLayout';
import { useGetTransaction } from '~/hooks/useGetTransaction';
import { Banner } from '~/ui/Banner';
import { PageHeader } from '~/ui/PageHeader';
import { StatusIcon } from '~/ui/StatusIcon';

function TransactionResultPageHeader({
	transaction,
	error,
}: {
	transaction: SuiTransactionBlockResponse;
	error?: string;
}) {
	const txnKindName = transaction.transaction?.data.transaction?.kind;
	const txnDigest = transaction.digest;
	const txnStatus = transaction.effects?.status.status;

	const isProgrammableTransaction = txnKindName === 'ProgrammableTransaction';

	return (
		<PageHeader
			type="Transaction"
			title={txnDigest}
			subtitle={!isProgrammableTransaction ? txnKindName : undefined}
			error={error}
			before={
				<div className="flex h-18 w-18 min-w-18 items-center justify-center rounded-xl bg-white/50">
					<StatusIcon success={txnStatus === 'success'} />
				</div>
			}
		/>
	);
}

export default function TransactionResult() {
	const { id } = useParams();
	const { isLoading, isError, data } = useGetTransaction(id as string);
	const txError = data ? getExecutionStatusError(data) : undefined;

	return (
		<PageLayout
			gradientContent={
				data && {
					content: <TransactionResultPageHeader transaction={data} error={txError} />,
					size: 'md',
				}
			}
			error={txError}
			content={
				isLoading ? (
					<LoadingIndicator text="Loading..." />
				) : isError || !data ? (
					<Banner variant="error" spacing="lg" fullWidth>
						{!id
							? "Can't search for a transaction without a digest"
							: `Data could not be extracted for the following specified transaction ID: ${id}`}
					</Banner>
				) : (
					<TransactionView transaction={data} />
				)
			}
		/>
	);
}
