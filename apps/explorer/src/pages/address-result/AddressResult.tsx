// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import { isSuiNSName, useResolveSuiNSAddress, useResolveSuiNSName } from '@mysten/core';
import { useParams } from 'react-router-dom';

import { ErrorBoundary } from '../../components/error-boundary/ErrorBoundary';
import { OwnedCoins } from '~/components/OwnedCoins';
import { OwnedObjects } from '~/components/OwnedObjects';
import TransactionBlocksForAddress, {
	ADDRESS_FILTER_VALUES,
} from '~/components/TransactionBlocksForAddress/TransactionBlocksForAddress';
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
					<div className="flex items-center justify-between border-b border-gray-45 pb-5">
						<Heading color="gray-90" variant="heading4/semibold">
							Transaction Blocks
						</Heading>
					</div>
					<div className="flex w-full flex-col md:flex-row">
						<div className="flex w-full border-gray-45 md:border-r">
							<TransactionBlocksForAddress address={address} filter={ADDRESS_FILTER_VALUES.FROM} />
						</div>
						<div className="flex w-full md:ml-5">
							<TransactionBlocksForAddress address={address} filter={ADDRESS_FILTER_VALUES.TO} />
						</div>
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
