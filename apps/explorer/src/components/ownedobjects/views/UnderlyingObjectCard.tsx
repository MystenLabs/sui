// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import { useGetDynamicFieldObject } from '@mysten/core';
import { getObjectDisplay, getObjectFields, getObjectId } from '@mysten/sui.js';

import { SyntaxHighlighter } from '~/components/SyntaxHighlighter';
import DisplayBox from '~/components/displaybox/DisplayBox';
import { Banner } from '~/ui/Banner';
import { ObjectLink } from '~/ui/InternalLink';
import { LoadingSpinner } from '~/ui/LoadingSpinner';
import { Text } from '~/ui/Text';
import {
    parseImageURL,
    parseObjectType,
    extractName,
} from '~/utils/objectUtils';
import { trimStdLibPrefix } from '~/utils/stringUtils';

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
    const display = getObjectDisplay(data);
    const imgUrl = parseImageURL(display.data);

    const objectType = parseObjectType(data);
    const caption = extractName(display.data) || trimStdLibPrefix(objectType);

    // Get content inside <> and split by , to get underlying object types
    const underlyingObjectTypes = objectType
        ?.slice(objectType?.indexOf('<') + 1, objectType.indexOf('>'))
        .split(',');

    // Split the first object type by :: and if array length is > 1 then it is a underlying object
    const hasUnderlyingObject =
        underlyingObjectTypes?.[0].split('::').length > 1;
    return (
        <>
            <SyntaxHighlighter
                code={JSON.stringify(fieldsData, null, 2)}
                language="json"
            />

            {hasUnderlyingObject ? (
                <div className="mt-3 pt-3">
                    <Text variant="body/medium" color="steel-dark">
                        Underlying Object
                    </Text>
                    <div className="item-center mt-5 flex justify-start gap-1">
                        {imgUrl ? (
                            <div className="w-16">
                                <DisplayBox
                                    display={imgUrl}
                                    caption={caption}
                                />
                            </div>
                        ) : null}
                        <div className="flex flex-col items-start justify-center gap-1 break-all">
                            <ObjectLink objectId={getObjectId(data)} />
                            {underlyingObjectTypes.map((type) => (
                                <Text
                                    variant="body/medium"
                                    color="steel-dark"
                                    key={type}
                                >
                                    {type}
                                </Text>
                            ))}
                        </div>
                    </div>
                </div>
            ) : null}
        </>
    );
}
