// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import { getObjectDisplay } from '@mysten/sui.js';

import DisplayBox from '~/components/displaybox/DisplayBox';
import { useGetDynamicFieldObject } from '~/hooks/useGetDynamicFieldObject';
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
            <div className="mt-3 pt-3">
                <LoadingSpinner text="Loading data" />
            </div>
        );
    }

    if (isError || data.error || (isFetched && !data)) {
        return null;
    }
    const display = getObjectDisplay(data);
    const imgUrl = parseImageURL(display.data);
    const objectType = parseObjectType(data);
    const caption = extractName(display.data) || trimStdLibPrefix(objectType);

    return imgUrl ? (
        <div className="mt-3 pt-3">
            <Text variant="body/medium" color="steel-dark">
                Underlying Object
            </Text>
            <div className="item-center mt-5 flex justify-start gap-1">
                <div className="w-16">
                    <DisplayBox display={imgUrl} caption={caption} />
                </div>
                <div className="flex flex-col items-start justify-center gap-1 break-all">
                    <ObjectLink objectId={parentId} />
                    <Text variant="body/medium" color="steel-dark">
                        {caption}
                    </Text>
                </div>
            </div>
        </div>
    ) : null;
}
