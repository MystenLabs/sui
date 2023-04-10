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

function OwnedObject({ obj }: OwnedObjectTypes): JSX.Element {
    const displayMeta = getObjectDisplay(obj).data;
    const display = transformURL(parseImageURL(displayMeta));
    return (
        <div className="w-6/12 lg:flex lg:flex-wrap lg:justify-between">
            <div className="my-2 flex h-fit w-full items-center truncate break-all sm:my-[1vh]">
                <div className="mr-5 h-[60px] min-w-[60px] max-w-[60px]">
                    <DisplayBox display={display} />
                </div>
                <div className="overflow-hidden sm:pr-5">
                    <Text variant="body/medium" color="gray-90" truncate>
                        {extractName(displayMeta)}
                    </Text>
                    <div>
                        <ObjectLink objectId={getObjectId(obj)} />
                    </div>
                    <div className="overflow-hidden text-gray-80">
                        <Text variant="p2/medium" truncate>
                            {trimStdLibPrefix(parseObjectType(obj))}
                        </Text>
                    </div>
                </div>
            </div>
        </div>
    );
}

export default OwnedObject;
