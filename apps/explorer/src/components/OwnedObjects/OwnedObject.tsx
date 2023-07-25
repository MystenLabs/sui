// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import { type SuiObjectResponse, getObjectDisplay } from '@mysten/sui.js';

import { useResolveVideo } from '~/hooks/useResolveVideo';
import { ObjectDetails } from '~/ui/ObjectDetails';
import { parseObjectType } from '~/utils/objectUtils';
import { trimStdLibPrefix } from '~/utils/stringUtils';

type OwnedObjectTypes = {
	obj: SuiObjectResponse;
};

export default function OwnedObject({ obj }: OwnedObjectTypes) {
	const video = useResolveVideo(obj);
	const displayMeta = getObjectDisplay(obj).data;

	return (
		<ObjectDetails
			variant="small"
			id={obj.data?.objectId}
			type={trimStdLibPrefix(parseObjectType(obj))}
			name={displayMeta?.name ?? displayMeta?.description}
			image={displayMeta?.image_url}
			video={video}
		/>
	);
}
