// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0
import { useGetObject, useResolveSuiNSName } from '@mysten/core';
import { translate } from '~/pages/object-result/ObjectResultType';
import { PACKAGE_TYPE_NAME } from '~/pages/id/PageContent';
import { PageHeader } from '~/ui/PageHeader';
import { ObjectDetailsHeader } from '@mysten/icons';
import { PackageDetails } from '~/pages/id/PackageDetails';
import { TotalStaked } from '~/pages/id/TotalStaked';
import { ErrorBoundary } from '~/components/error-boundary/ErrorBoundary';
import { ObjectView } from '~/pages/object-result/views/ObjectView';

export function Header({ address, loading }: { address: string; loading?: boolean }) {
	const { data: domainName, isLoading } = useResolveSuiNSName(address);
	const { data, isPending, error: getObjectError } = useGetObject(address);
	const isObject = !!data?.data;
	const resp = data && isObject && !getObjectError ? translate(data) : null;
	const isPackage = resp ? resp.objType === PACKAGE_TYPE_NAME : false;
	const pageType = isPackage ? 'Package' : isObject ? 'Object' : 'Address';

	return (
		<div>
			<PageHeader
				error={getObjectError?.message}
				loading={loading || isLoading || isPending}
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
