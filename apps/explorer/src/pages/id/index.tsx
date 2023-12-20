// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import { useParams } from 'react-router-dom';
import {
	isSuiNSName,
	useGetObject,
	useResolveSuiNSAddress,
	useResolveSuiNSName,
} from '@mysten/core';
import { PageLayout } from '~/components/Layout/PageLayout';
import { PageHeader } from '~/ui/PageHeader';
import { ObjectDetailsHeader } from '@mysten/icons';
import { TotalStaked } from './TotalStaked';
import { ErrorBoundary } from '~/components/error-boundary/ErrorBoundary';
import { ObjectView } from '~/pages/object-result/views/ObjectView';
import { PACKAGE_TYPE_NAME, PageContent } from './PageContent';
import { translate } from '~/pages/object-result/ObjectResultType';
import { PackageDetails } from '~/pages/id/PackageDetails';
import { type SuiObjectResponse } from '@mysten/sui.js/dist/cjs/client';
import { Banner } from '~/ui/Banner';

function Header({
	address,
	domainName,
	loading,
	error,
	data,
}: {
	address: string;
	loading?: boolean;
	domainName?: string | null;
	error?: Error | null;
	data?: SuiObjectResponse | null;
}) {
	const isObject = !!data?.data;
	const resp = data && isObject && !error ? translate(data) : null;
	const isPackage = resp ? resp.objType === PACKAGE_TYPE_NAME : false;
	const pageType = isPackage ? 'Package' : isObject ? 'Object' : 'Address';

	return (
		<div>
			<PageHeader
				error={error?.message}
				loading={loading}
				type={pageType}
				title={address}
				subtitle={domainName}
				before={<ObjectDetailsHeader className="h-6 w-6" />}
				after={
					pageType === 'Package' && resp ? (
						<PackageDetails data={resp} />
					) : (
						<TotalStaked address={address} />
					)
				}
			/>

			<ErrorBoundary>
				{pageType !== 'Package' && data && (
					<div className="mt-5">
						<ObjectView data={data} />
					</div>
				)}
			</ErrorBoundary>
		</div>
	);
}

function PageLayoutContainer() {
	const { id } = useParams();
	const isSuiNSAddress = isSuiNSName(id!);
	const {
		data: resolvedAddress,
		isLoading: loadingResolveSuiNSAddress,
		error: resolveSuinsAddressError,
	} = useResolveSuiNSAddress(id, isSuiNSAddress);

	const {
		data: domainName,
		isLoading: loadingDomainName,
		error: domainNameError,
	} = useResolveSuiNSName(id, !isSuiNSAddress);

	const { data, isPending, error: getObjectError } = useGetObject(id!);

	const error = resolveSuinsAddressError || domainNameError || getObjectError;
	const loading = isPending || loadingResolveSuiNSAddress || loadingDomainName;
	const displayAddress = resolvedAddress || id!;

	return (
		<PageLayout
			loading={loading}
			isError={!!error}
			gradient={{
				size: 'md',
				content: (
					<Header
						address={id!}
						error={error}
						loading={loading}
						domainName={domainName}
						data={data}
					/>
				),
			}}
			content={
				!loading && error ? (
					<Banner variant="error" spacing="lg" fullWidth>
						Data could not be extracted on the following specified address ID: {displayAddress}
					</Banner>
				) : (
					<PageContent address={displayAddress} data={data} />
				)
			}
		/>
	);
}

export function Id() {
	return <PageLayoutContainer />;
}
