// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import { useGetDynamicFields, useOnScreen } from '@mysten/core';
import { type DynamicFieldInfo } from '@mysten/sui.js/client';
import { LoadingIndicator } from '@mysten/ui';
import { useRef, useEffect, useState, useMemo } from 'react';

import { UnderlyingObjectCard } from './UnderlyingObjectCard';
import { FieldsCard, FieldCollapsible, FieldsContainer } from '~/components/Object/FieldsUtils';
import { ObjectLink } from '~/ui/InternalLink';

function DynamicFieldRow({
	id,
	result,
	noMarginBottom,
	defaultOpen,
}: {
	id: string;
	result: DynamicFieldInfo;
	noMarginBottom: boolean;
	defaultOpen: boolean;
}) {
	const [open, onOpenChange] = useState(defaultOpen);

	return (
		<FieldCollapsible
			open={open}
			onOpenChange={onOpenChange}
			noMarginBottom={noMarginBottom}
			name={
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
		>
			<div className="flex flex-col divide-y divide-gray-45">
				<UnderlyingObjectCard parentId={id} name={result.name} dynamicFieldType={result.type} />
			</div>
		</FieldCollapsible>
	);
}

export function DynamicFieldsCard({ id }: { id: string }) {
	const { data, isInitialLoading, isFetchingNextPage, hasNextPage, fetchNextPage } =
		useGetDynamicFields(id);

	const observerElem = useRef<HTMLDivElement | null>(null);
	const { isIntersecting } = useOnScreen(observerElem);
	const isSpinnerVisible = isFetchingNextPage && hasNextPage;
	const flattenedData = useMemo(() => data?.pages.flatMap((page) => page.data), [data]);

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

	return (
		<FieldsContainer>
			<FieldsCard>
				{flattenedData?.map((result, index) => (
					<DynamicFieldRow
						key={result.objectId}
						defaultOpen={index === 0}
						noMarginBottom={index === flattenedData.length - 1}
						id={id}
						result={result}
					/>
				))}

				<div ref={observerElem}>
					{isSpinnerVisible ? (
						<div className="mt-1 flex w-full justify-center">
							<LoadingIndicator text="Loading data" />
						</div>
					) : null}
				</div>
			</FieldsCard>
		</FieldsContainer>
	);
}
