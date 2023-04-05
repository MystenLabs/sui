// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import {
    type SuiObjectResponse,
    getObjectId,
    getObjectDisplay,
} from '@mysten/sui.js';

import DisplayBox from '~/components/displaybox/DisplayBox';
import { ObjectLink } from '~/ui/InternalLink';
import { Text } from '~/ui/Text';
import {
    extractName,
    parseImageURL,
    parseObjectType,
} from '~/utils/objectUtils';
import { transformURL, trimStdLibPrefix } from '~/utils/stringUtils';

type OwnedObjectTypes = {
    obj: SuiObjectResponse;
};

function OwnedObject({ obj }: OwnedObjectTypes) {
    const displayMeta = getObjectDisplay(obj).data;
    const display = transformURL(parseImageURL(displayMeta));
    return (
        <div
            id="ownedObject"
            className="w-[50%] lg:flex lg:flex-wrap lg:justify-between"
        >
            <div className="my-2 flex h-fit min-h-[72px] w-[100%] items-center overflow-x-hidden text-ellipsis whitespace-nowrap break-all sm:my-[1vh]">
                <div className="mr-[20px] h-[60px] min-w-[60px] max-w-[60px]">
                    <DisplayBox display={display} />
                </div>
                <div className="overflow-hidden sm:pr-[20px]">
                    <div className="overflow-hidden text-ellipsis whitespace-nowrap text-[13px] font-medium leading-[130%] text-gray-90">
                        {extractName(displayMeta)}
                    </div>
                    <div>
                        <ObjectLink objectId={getObjectId(obj)} />
                    </div>
                    <div className="overflow-hidden text-gray-80">
                        <Text variant="p2/medium" hideOverflow>
                            {trimStdLibPrefix(parseObjectType(obj))}
                        </Text>
                    </div>
                </div>
            </div>
        </div>
    );
}

export default OwnedObject;
