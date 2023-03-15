// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import {
    type SuiObjectResponse,
    getObjectId,
    getObjectDisplay,
} from '@mysten/sui.js';

import useImage from '~/hooks/useImage';
import { ObjectDetails } from '~/ui/ObjectDetails';
import { extractName, parseObjectType } from '~/utils/objectUtils';

type OwnedObjectTypes = {
    obj: SuiObjectResponse;
};

function OwnedObject({ obj }: OwnedObjectTypes): JSX.Element {
    const displayMeta = getObjectDisplay(obj).data;
    const { url } = useImage({ src: displayMeta?.image_url });
    return (
        <ObjectDetails
            id={getObjectId(obj)}
            name={extractName(displayMeta) ?? ''}
            variant="small"
            type={parseObjectType(obj)}
            image={url}
        />
    );
}

export default OwnedObject;
