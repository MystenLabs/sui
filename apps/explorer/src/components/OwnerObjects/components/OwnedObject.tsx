// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import DisplayBox from '~/components/displaybox/DisplayBox';
import { ObjectLink } from '~/ui/InternalLink';
import { Text } from '~/ui/Text';
import { transformURL, trimStdLibPrefix } from '~/utils/stringUtils';
import { SuiObjectResponse, getObjectId } from '@mysten/sui.js';
import { extractName, parseImageURL, parseObjectType } from '~/utils/objectUtils';

type OwnedObjectTypes = {
    obj: SuiObjectResponse
}

const OwnedObject = ({ obj }: OwnedObjectTypes) => {
    const display = transformURL(parseImageURL(obj.data?.display)) ?? '';
    return (
        <div id="ownedObject" className="w-[50%] lg:flex lg:flex-wrap lg:justify-between">
            <div className="my-2 flex h-fit min-h-[72px] items-center break-all sm:my-[1vh] w-[100%] overflow-x-hidden whitespace-nowrap text-ellipsis">
                <div className="h-[60px] min-w-[60px] max-w-[60px] mr-[20px]">
                    <DisplayBox display={display} />
                </div>
                <div className="sm:pr-[20px] overflow-hidden">
                    <div className="text-[13px] font-medium leading-[130%] text-gray-90 overflow-hidden whitespace-nowrap text-ellipsis">
                        {extractName(obj.data?.display)}
                    </div>
                    <div>
                        <ObjectLink objectId={getObjectId(obj)} />
                    </div>
                    <div className="overflow-hidden text-gray-80">
                        <Text variant="p2/medium" hideOverflow={true}>
                            {trimStdLibPrefix(parseObjectType(obj))}
                        </Text>
                    </div>
                </div>
            </div>
        </div>
    );
}

export default OwnedObject