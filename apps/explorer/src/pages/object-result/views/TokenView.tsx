// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import { useGetDynamicFields, useGetObject } from '@mysten/core';
import { useSuiClientQuery } from '@mysten/dapp-kit';
import { type SuiObjectResponse } from '@mysten/sui.js/client';
import { Heading } from '@mysten/ui';
import { type ReactNode, useState } from 'react';

import { DynamicFieldsCard } from '~/components/Object/DynamicFieldsCard';
import { ObjectFieldsCard } from '~/components/Object/ObjectFieldsCard';
import TransactionBlocksForAddress from '~/components/TransactionBlocksForAddress/TransactionBlocksForAddress';
import { Tabs, TabsContent, TabsList, TabsTrigger } from '~/ui/Tabs';
import { OwnedCoins } from '~/components/OwnedCoins';
import { OwnedObjects } from '~/components/OwnedObjects';
import { LOCAL_STORAGE_SPLIT_PANE_KEYS, SplitPanes } from '~/ui/SplitPanes';
import { Divider } from '~/ui/Divider';
import { ErrorBoundary } from '~/components/error-boundary/ErrorBoundary';
import { useBreakpoint } from '~/hooks/useBreakpoint';

const LEFT_RIGHT_PANEL_MIN_SIZE = 30;
const PANEL_MIN_SIZE = 20;

function FieldsContainer({ children }: { children: ReactNode }) {
	return (
		<div className="mt-4 flex flex-col gap-5 rounded-xl border border-gray-45 bg-objectCard py-6 pl-6 pr-4">
			{children}
		</div>
	);
}

enum TABS_VALUES {
	FIELDS = 'fields',
	DYNAMIC_FIELDS = 'dynamicFields',
}

function useObjectFieldsCard(id: string) {
	const { data: suiObjectResponseData, isPending, isError } = useGetObject(id);

	const objectType =
		suiObjectResponseData?.data?.type ??
		suiObjectResponseData?.data?.content?.dataType === 'package'
			? suiObjectResponseData.data.type
			: suiObjectResponseData?.data?.content?.type;

	const [packageId, moduleName, functionName] = objectType?.split('<')[0]?.split('::') || [];

	// Get the normalized struct for the object
	const {
		data: normalizedStructData,
		isPending: loadingNormalizedStruct,
		isError: errorNormalizedMoveStruct,
	} = useSuiClientQuery(
		'getNormalizedMoveStruct',
		{
			package: packageId,
			module: moduleName,
			struct: functionName,
		},
		{
			enabled: !!packageId && !!moduleName && !!functionName,
		},
	);

	return {
		loading: isPending || loadingNormalizedStruct,
		error: isError || errorNormalizedMoveStruct,
		normalizedStructData,
		suiObjectResponseData,
		objectType,
	};
}

export function TokenView({ data }: { data: SuiObjectResponse }) {
	const isMediumOrAbove = useBreakpoint('md');
	const objectId = data.data?.objectId!;

	const {
		normalizedStructData,
		suiObjectResponseData,
		objectType,
		loading: objectFieldsCardLoading,
		error: objectFieldsCardError,
	} = useObjectFieldsCard(objectId);

	const fieldsCount = normalizedStructData?.fields.length;

	const [activeTab, setActiveTab] = useState<string>(TABS_VALUES.FIELDS);

	const { data: dynamicFieldsData } = useGetDynamicFields(objectId);

	const renderDynamicFields = !!dynamicFieldsData?.pages?.[0].data.length;

	const leftPane = {
		panel: <OwnedCoins id={objectId} />,
		minSize: LEFT_RIGHT_PANEL_MIN_SIZE,
		defaultSize: LEFT_RIGHT_PANEL_MIN_SIZE,
	};

	const rightPane = {
		panel: <OwnedObjects id={objectId} />,
		minSize: LEFT_RIGHT_PANEL_MIN_SIZE,
	};

	const topPane = {
		panel: (
			<div className="flex h-full flex-col justify-between">
				<ErrorBoundary>
					{isMediumOrAbove ? (
						<SplitPanes
							autoSaveId={LOCAL_STORAGE_SPLIT_PANE_KEYS.OBJECT_VIEW_HORIZONTAL}
							dividerSize="none"
							splitPanels={[leftPane, rightPane]}
							direction="horizontal"
						/>
					) : (
						<>
							{leftPane.panel}
							<div className="my-8">
								<Divider />
							</div>
							{rightPane.panel}
						</>
					)}
				</ErrorBoundary>
			</div>
		),
		minSize: PANEL_MIN_SIZE,
	};

	const middlePane = {
		panel: (
			<div className="flex h-full flex-col flex-nowrap gap-14 overflow-auto pt-12">
				<Tabs size="lg" value={activeTab} onValueChange={setActiveTab}>
					<TabsList>
						<TabsTrigger value={TABS_VALUES.FIELDS}>
							<Heading variant="heading4/semibold">{fieldsCount} Fields</Heading>
						</TabsTrigger>

						{renderDynamicFields && (
							<TabsTrigger value={TABS_VALUES.DYNAMIC_FIELDS}>
								<Heading variant="heading4/semibold">Dynamic Fields</Heading>
							</TabsTrigger>
						)}
					</TabsList>

					<TabsContent value={TABS_VALUES.FIELDS}>
						<FieldsContainer>
							<ObjectFieldsCard
								objectType={objectType || ''}
								normalizedStructData={normalizedStructData}
								suiObjectResponseData={suiObjectResponseData}
								loading={objectFieldsCardLoading}
								error={objectFieldsCardError}
								id={objectId}
							/>
						</FieldsContainer>
					</TabsContent>
					{renderDynamicFields && (
						<TabsContent value={TABS_VALUES.DYNAMIC_FIELDS}>
							<FieldsContainer>
								<DynamicFieldsCard id={objectId} />
							</FieldsContainer>
						</TabsContent>
					)}
				</Tabs>
			</div>
		),
		minSize: PANEL_MIN_SIZE,
	};

	const bottomPane = {
		panel: (
			<div className="flex h-full flex-col flex-nowrap gap-14 overflow-auto pt-12">
				<TransactionBlocksForAddress address={objectId} isObject />
			</div>
		),
		minSize: PANEL_MIN_SIZE,
	};

	return (
		<>
			{isMediumOrAbove ? (
				<div className="h-300">
					<SplitPanes
						autoSaveId={LOCAL_STORAGE_SPLIT_PANE_KEYS.ADDRESS_VIEW_VERTICAL}
						dividerSize="none"
						splitPanels={[topPane, middlePane, bottomPane]}
						direction="vertical"
					/>
				</div>
			) : (
				<>
					{topPane.panel}
					<div className="mt-5">
						<Divider />
					</div>
					{middlePane.panel}
					<div className="mt-5">
						<Divider />
					</div>
					{bottomPane.panel}
				</>
			)}
		</>
	);
}
