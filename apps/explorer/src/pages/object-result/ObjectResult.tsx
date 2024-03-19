// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import { useGetObject } from '@mysten/core';
import { ObjectDetailsHeader } from '@mysten/icons';
import { LoadingIndicator } from '@mysten/ui';
import clsx from 'clsx';
import { useParams } from 'react-router-dom';

import { translate, type DataType } from './ObjectResultType';
import PkgView from './views/PkgView';
import { TokenView } from './views/TokenView';
import { PageLayout } from '~/components/Layout/PageLayout';
import { ErrorBoundary } from '~/components/error-boundary/ErrorBoundary';
import { ObjectView } from '~/pages/object-result/views/ObjectView';
import { Banner } from '~/ui/Banner';
import { PageHeader } from '~/ui/PageHeader';

const PACKAGE_TYPE_NAME = 'Move Package';

export function ObjectResult() {
	const { id: objID } = useParams();
	const { data, isPending, isError, isFetched } = useGetObject(objID!);

	if (isPending) {
		return (
			<PageLayout
				content={
					<div className="flex w-full items-center justify-center">
						<LoadingIndicator text="Loading data" />
					</div>
				}
			/>
		);
	}

	const isPageError = isError || data.error || (isFetched && !data);

	const resp = data && !isPageError ? translate(data) : null;
	const isPackage = resp ? resp.objType === PACKAGE_TYPE_NAME : false;

	return (
		<PageLayout
			isError={!!isPageError}
			gradient={
				isPackage
					? undefined
					: {
							size: 'md',
							content: (
								<div>
									<PageHeader
										type="Object"
										title={resp?.id ?? ''}
										before={<ObjectDetailsHeader className="h-6 w-6" />}
									/>

									<ErrorBoundary>
										{data && (
											<div className="mt-5">
												<ObjectView data={data} />
											</div>
										)}
									</ErrorBoundary>
								</div>
							),
					  }
			}
			content={
				<>
					{isPageError || !data || !resp ? (
						<Banner variant="error" spacing="lg" fullWidth>
							Data could not be extracted on the following specified object ID: {objID}
						</Banner>
					) : (
						<div className="mb-10">
							{isPackage && <PageHeader type="Package" title={resp.id} />}
							<ErrorBoundary>
								<div className={clsx(isPackage && 'mt-10')}>
									{isPackage ? <PkgView data={resp} /> : <TokenView data={data} />}
								</div>
							</ErrorBoundary>
						</div>
					)}
				</>
			}
		/>
	);
}

export type { DataType };
