// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import { isSuiNSName, useResolveSuiNSAddress, useResolveSuiNSName } from '@mysten/core';
import { useParams } from 'react-router-dom';

import { ErrorBoundary } from '../../components/error-boundary/ErrorBoundary';
import { TransactionsForAddress } from '../../components/transactions/TransactionsForAddress';

import { OwnedCoins } from '~/components/OwnedCoins';
import { OwnedObjects } from '~/components/OwnedObjects';
import { Heading } from '~/ui/Heading';
import { LoadingSpinner } from '~/ui/LoadingSpinner';
import { PageHeader } from '~/ui/PageHeader';

function AddressResult({ address }: { address: string }) {
	const { data: domainName } = useResolveSuiNSName(address);

	return (
		<div className="space-y-12">
			<PageHeader type="Address" title={address} subtitle={domainName} />
			<div>
				<div className="border-b border-gray-45 pb-5 md:mt-12">
					<Heading color="gray-90" variant="heading4/semibold">
						Owned Objects
					</Heading>
				</div>
				<ErrorBoundary>
					<div className="flex flex-col gap-10 md:flex-row">
						<div className="flex-1 overflow-hidden">
							<OwnedCoins id={address} />
						</div>
						<div className="hidden w-px bg-gray-45 md:block" />
						<div className="flex-1 overflow-hidden">
							<OwnedObjects id={address} />
						</div>
					</div>
				</ErrorBoundary>
			</div>

			<div>
				<ErrorBoundary>
					<div className="mt-2">
						<TransactionsForAddress address={address} type="address" />
					</div>
				</ErrorBoundary>
			</div>
		</div>
	);
}

function SuiNSAddressResult({ name }: { name: string }) {
	const { isFetched, data } = useResolveSuiNSAddress(name);

	if (!isFetched) {
		return <LoadingSpinner />;
	}

	// Fall back into just trying to load the name as an address anyway:
	return <AddressResult address={data ?? name} />;
}

export default function AddressResultPage() {
	const { id } = useParams();

	if (isSuiNSName(id!)) {
		return <SuiNSAddressResult name={id!} />;
	}

	return <AddressResult address={id!} />;
}
