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
import { PageContent } from './PageContent';
import { type DataType, translate } from '~/pages/object-result/ObjectResultType';
import { PackageDetails } from '~/pages/id-page/PackageDetails';
import { type SuiObjectResponse } from '@mysten/sui.js/dist/cjs/client';

const PACKAGE_TYPE_NAME = 'Move Package';

function Header({
	pageType,
	address,
	domainName,
	loading,
	error,
	data,
}: {
	pageType: 'Package' | 'Object' | 'Address';
	address: string;
	loading?: boolean;
	domainName?: string | null;
	error?: Error | null;
	data?: DataType | SuiObjectResponse | null;
}) {
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
					pageType === 'Package' && data && 'id' in data ? (
						<PackageDetails data={data} />
					) : (
						<TotalStaked address={address} />
					)
				}
			/>

			<ErrorBoundary>
				{data && pageType !== 'Package' && data && !('id' in data) && (
					<div className="mt-5">
						<ObjectView data={data} />
					</div>
				)}
			</ErrorBoundary>
		</div>
	);
}

function PageLayoutContainer({ address }: { address: string }) {
	const { id } = useParams();
	const isSuiNSAddress = isSuiNSName(id!);
	const {
		data: resolvedAddress,
		isLoading: loadingResolveSuiNSAddress,
		error: resolveSuinsAddressError,
	} = useResolveSuiNSAddress(address, isSuiNSAddress);

	const {
		data: domainName,
		isLoading: loadingDomainName,
		error: domainNameError,
	} = useResolveSuiNSName(address, !isSuiNSAddress);

	const { data, isPending, error: getObjectError } = useGetObject(address!);

	const isObject = !!data?.data;
	const error = resolveSuinsAddressError || domainNameError || getObjectError;
	const resp = data && isObject && !error ? translate(data) : null;
	const isPackage = resp ? resp.objType === PACKAGE_TYPE_NAME : false;
	const loading = isPending || loadingResolveSuiNSAddress || loadingDomainName;
	const pageType = isPackage ? 'Package' : isObject ? 'Object' : 'Address';

	return (
		<PageLayout
			loading={loading}
			isError={!!error}
			gradient={{
				size: 'md',
				content: (
					<Header
						address={address}
						pageType={pageType}
						error={error}
						loading={loading}
						domainName={domainName}
						data={pageType === 'Package' ? resp : data}
					/>
				),
			}}
			content={
				<PageContent
					address={resolvedAddress || address}
					error={error}
					pageType={pageType}
					data={resp}
				/>
			}
		/>
	);
}

export function IdPage() {
	const { id } = useParams();

	return <PageLayoutContainer address={id!} />;
}
