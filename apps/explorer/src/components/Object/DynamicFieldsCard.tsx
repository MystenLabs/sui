// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import { useGetDynamicFields, useOnScreen } from '@mysten/core';
import { LoadingIndicator } from '@mysten/ui';
import { useRef, useEffect } from 'react';

import { UnderlyingObjectCard } from './UnderlyingObjectCard';
import { DisclosureBox } from '~/ui/DisclosureBox';
import { ObjectLink } from '~/ui/InternalLink';
import { TabHeader } from '~/ui/Tabs';

export function DynamicFieldsCard({ id }: { id: string }) {
	const { data, isInitialLoading, isFetchingNextPage, hasNextPage, fetchNextPage } =
		useGetDynamicFields(id);

	const observerElem = useRef<HTMLDivElement | null>(null);
	const { isIntersecting } = useOnScreen(observerElem);
	const isSpinnerVisible = isFetchingNextPage && hasNextPage;

	useEffect(() => {
		if (isIntersecting && hasNextPage && !isFetchingNextPage) {
			fetchNextPage();
		}
	}, [isIntersecting, fetchNextPage, hasNextPage, isFetchingNextPage]);

	if (isInitialLoading) {
		return (
			<div className="mt-1 flex w-full justify-center">
				<LoadingIndicator />
			</div>
		);
	}

	// show the dynamic fields tab if there are pages and the first page has data
	const hasPages = !!data?.pages?.[0].data.length;

	return hasPages ? (
		<div className="mt-10">
			<TabHeader title="Dynamic Fields">
				<div className="mt-4 flex max-h-600 flex-col gap-5 overflow-auto">
					{data.pages.map(({ data }) =>
						// Show the field name and type is it is not an object
						data.map((result) => (
							<DisclosureBox
								title={
									<div className="flex items-center gap-1 truncate break-words text-body font-medium leading-relaxed text-steel-dark">
										<div className="block w-full truncate break-words">
											{typeof result.name?.value === 'object' ? (
												<>Struct {result.name.type}</>
											) : result.name?.value ? (
												String(result.name.value)
											) : null}
										</div>
										<ObjectLink objectId={result.objectId} />
									</div>
								}
								variant="outline"
								key={result.objectId}
							>
								<div className="flex flex-col divide-y divide-gray-45">
									<UnderlyingObjectCard
										parentId={id}
										name={result.name}
										dynamicFieldType={result.type}
									/>
								</div>
							</DisclosureBox>
						)),
					)}

					<div ref={observerElem}>
						{isSpinnerVisible ? (
							<div className="mt-1 flex w-full justify-center">
								<LoadingIndicator text="Loading data" />
							</div>
						) : null}
					</div>
				</div>
			</TabHeader>
		</div>
	) : null;
}
