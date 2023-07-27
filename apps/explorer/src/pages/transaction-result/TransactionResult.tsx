// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import { getExecutionStatusError, type SuiTransactionBlockResponse } from '@mysten/sui.js';
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
	loading,
}: {
	transaction?: SuiTransactionBlockResponse;
	error?: string;
	loading?: boolean;
}) {
	const txnKindName = transaction?.transaction?.data.transaction?.kind;
	const txnDigest = transaction?.digest ?? '';
	const txnStatus = transaction?.effects?.status.status;

	const isProgrammableTransaction = txnKindName === 'ProgrammableTransaction';

	return (
		<PageHeader
			loading={loading}
			type="Transaction"
			title={txnDigest}
			subtitle={!isProgrammableTransaction ? txnKindName : undefined}
			error={error}
			before={<StatusIcon success={txnStatus === 'success'} />}
		/>
	);
}

export default function TransactionResult() {
	const { id } = useParams();
	const { isLoading, isError, data } = useGetTransaction(id as string);
	const txError = data ? getExecutionStatusError(data) : undefined;

	return (
		<PageLayout
			loading={isLoading}
			gradient={{
				content: (
					<TransactionResultPageHeader transaction={data} error={txError} loading={isLoading} />
				),
				size: 'md',
				type: txError ? 'error' : 'success',
			}}
			content={
				isError || !data ? (
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
