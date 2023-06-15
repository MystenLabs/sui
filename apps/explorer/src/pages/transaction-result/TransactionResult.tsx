// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import { useParams } from 'react-router-dom';

import { TransactionView } from './TransactionView';

import { useGetTransaction } from '~/hooks/useGetTransaction';
import { Banner } from '~/ui/Banner';
import { LoadingSpinner } from '~/ui/LoadingSpinner';

export default function TransactionResult() {
	const { id } = useParams();
	const { isLoading, isError, data } = useGetTransaction(id as string);

	// TODO update Loading screen
	if (isLoading) {
		return <LoadingSpinner text="Loading..." />;
	}

	if (isError || !data) {
		return (
			<Banner variant="error" spacing="lg" fullWidth>
				{!id
					? "Can't search for a transaction without a digest"
					: `Data could not be extracted for the following specified transaction ID: ${id}`}
			</Banner>
		);
	}

	return <TransactionView transaction={data} />;
}
