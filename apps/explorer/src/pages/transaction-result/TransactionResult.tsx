// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import { LoadingIndicator } from '@mysten/ui';
import { useParams } from 'react-router-dom';

import { TransactionView } from './TransactionView';
import { PageLayout } from '~/components/Layout/PageLayout';
import { useGetTransaction } from '~/hooks/useGetTransaction';
import { Banner } from '~/ui/Banner';

export default function TransactionResult() {
	const { id } = useParams();
	const { isLoading, isError, data } = useGetTransaction(id as string);
	return (
		<PageLayout
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
