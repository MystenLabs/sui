// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import { useGetDynamicFieldObject, useGetNormalizedMoveStruct } from '@mysten/core';
import { getObjectFields, getObjectType } from '@mysten/sui.js';
import { LoadingIndicator } from '@mysten/ui';

import { FieldItem } from './FieldItem';
import { Banner } from '~/ui/Banner';

interface UnderlyingObjectCardProps {
	parentId: string;
	name: {
		type: string;
		value?: string;
	};
	dynamicFieldType: 'DynamicField' | 'DynamicObject';
}

export function UnderlyingObjectCard({
	parentId,
	name,
	dynamicFieldType,
}: UnderlyingObjectCardProps) {
	const { data, isLoading, isError, isFetched } = useGetDynamicFieldObject(parentId, name);
	const objectType = data ? getObjectType(data!) : null;
	// Get the packageId, moduleName, functionName from the objectType
	const [packageId, moduleName, functionName] = objectType?.split('<')[0]?.split('::') || [];

	// Get the normalized struct for the object
	const {
		data: normalizedStruct,
		isFetched: normalizedStructFetched,
		isLoading: loadingNormalizedStruct,
	} = useGetNormalizedMoveStruct({
		packageId,
		module: moduleName,
		struct: functionName,
	});

	// Check for error first before showing the loading spinner to avoid infinite loading if GetDynamicFieldObject fails
	if (
		isError ||
		(data && data.error) ||
		(isFetched && !data) ||
		(!normalizedStruct && normalizedStructFetched)
	) {
		return (
			<Banner variant="error" spacing="lg" fullWidth>
				Failed to get field data for {parentId}
			</Banner>
		);
	}

	if (isLoading || loadingNormalizedStruct) {
		return (
			<div className="mt-3 flex w-full justify-center pt-3">
				<LoadingIndicator text="Loading data" />
			</div>
		);
	}

	const fieldsData = getObjectFields(data);
	// Return null if there are no fields
	if (!fieldsData || !normalizedStruct?.fields || !objectType) {
		return null;
	}
	// For dynamicObject type show the entire object
	const fieldData = dynamicFieldType === 'DynamicObject' ? fieldsData : fieldsData?.value;

	const dynamicFieldsData =
		// show name if it is a struct
		typeof name.value === 'object' ? { name, value: fieldData } : fieldData;

	return (
		<FieldItem
			value={dynamicFieldsData}
			objectType={objectType}
			// add the struct type to the value
			type={normalizedStruct?.fields.find((field) => field.name === 'value')?.type || ''}
		/>
	);
}
