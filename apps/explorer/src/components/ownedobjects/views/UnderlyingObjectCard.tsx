// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import { useGetDynamicFieldObject } from '@mysten/core';
import { getObjectFields } from '@mysten/sui.js';

import { SyntaxHighlighter } from '~/components/SyntaxHighlighter';
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

    if (isLoading) {
        return (
            <div className="mt-3 flex w-full justify-center pt-3">
                <LoadingSpinner text="Loading data" />
            </div>
        );
    }

    if (isError || data.error || (isFetched && !data)) {
        return (
            <Banner variant="error" spacing="lg" fullWidth>
                Failed to get field data for :{parentId}
            </Banner>
        );
    }

    const fieldsData = getObjectFields(data);
    return fieldsData ? (
        <SyntaxHighlighter
            code={JSON.stringify(
                typeof fieldsData?.name === 'object'
                    ? { name: fieldsData.name, value: fieldsData.value }
                    : fieldsData?.value,
                null,
                2
            )}
            language="json"
        />
    ) : null;
}
