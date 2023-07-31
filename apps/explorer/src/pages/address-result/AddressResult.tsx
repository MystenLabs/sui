// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import { isSuiNSName, useResolveSuiNSAddress, useResolveSuiNSName } from '@mysten/core';
import { Domain32 } from '@mysten/icons';
import { Heading, LoadingIndicator } from '@mysten/ui';
import { useParams } from 'react-router-dom';

import { ErrorBoundary } from '../../components/error-boundary/ErrorBoundary';
import { TransactionsForAddress } from '../../components/transactions/TransactionsForAddress';
import { PageLayout } from '~/components/Layout/PageLayout';
import { OwnedCoins } from '~/components/OwnedCoins';
import { OwnedObjects } from '~/components/OwnedObjects';
import { PageHeader } from '~/ui/PageHeader';

function AddressResult({ address }: { address: string }) {
	return (
		<div className="space-y-12">
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
		return <LoadingIndicator />;
	}

	// Fall back into just trying to load the name as an address anyway:
	return <AddressResult address={data ?? name} />;
}

export default function AddressResultPage() {
	const { id } = useParams();
	const { data: domainName } = useResolveSuiNSName(id);

	return (
		<PageLayout
			gradient={{
				size: 'md',
				content: (
					<PageHeader
						type="Address"
						title={id || ''}
						subtitle={domainName}
						before={<Domain32 className="h-10 w-10" />}
					/>
				),
			}}
			content={
				isSuiNSName(id!) ? <SuiNSAddressResult name={id!} /> : <AddressResult address={id!} />
			}
		/>
	);
}
