// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import {
    useGetDynamicFieldObject,
    useGetNormalizedMoveStruct,
} from '@mysten/core';
import { getObjectFields, getObjectType } from '@mysten/sui.js';

import { FieldItem } from './FieldItem';

import { Banner } from '~/ui/Banner';
import { LoadingSpinner } from '~/ui/LoadingSpinner';

interface UnderlyingObjectCardProps {
    parentId: string;
    name: {
        type: string;
        value?: string;
    };
}

export function UnderlyingObjectCard({
    parentId,
    name,
}: UnderlyingObjectCardProps) {
    const { data, isLoading, isError, isFetched } = useGetDynamicFieldObject(
        parentId,
        name
    );
    const objectType = data ? getObjectType(data!) : null;
    // Get the packageId, moduleName, functionName from the objectType
    const [packageId, moduleName, functionName] =
        objectType?.split('<')[0]?.split('::') || [];

    // Get the normalized struct for the object
    const { data: normalizedStruct, isLoading: loadingNormalizedStruct } =
        useGetNormalizedMoveStruct({
            packageId,
            module: moduleName,
            struct: functionName,
        });

    if (isLoading || loadingNormalizedStruct) {
        return (
            <div className="mt-3 flex w-full justify-center pt-3">
                <LoadingSpinner text="Loading data" />
            </div>
        );
    }

    if (isError || data.error || (isFetched && !data)) {
        return (
            <Banner variant="error" spacing="lg" fullWidth>
                Failed to get field data for {parentId}
            </Banner>
        );
    }

    const fieldsData = getObjectFields(data);
    // Return null if there are no fields
    if (!fieldsData || !normalizedStruct?.fields || !objectType) {
        return null;
    }
    return (
        <FieldItem
            value={
                // show name if it is a struct
                typeof fieldsData?.name === 'object'
                    ? { name: fieldsData.name, value: fieldsData.value }
                    : fieldsData?.value
            }
            objectType={objectType}
            // add the struct type to the value
            type={
                normalizedStruct?.fields.find((field) => field.name === 'value')
                    ?.type || ''
            }
        />
    );
}
