// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import { OwnedObjectType } from './OwnedObjects';
import { DisplayObject } from '../DisplayObject';
import { Button } from '../Base/Button';
import { KioskFnType } from '../../hooks/kiosk';
import { usePlaceMutation } from '../../mutations/kiosk';
import { ObjectId } from '@mysten/sui.js';

export function OwnedObject({
	object,
	onListSuccess,
	listFn,
	kioskId,
}: {
	onListSuccess: () => void;
	listFn: KioskFnType;
	object: OwnedObjectType;
	kioskId: ObjectId;
}) {
	const placeToKioskMutation = usePlaceMutation({
		onSuccess: onListSuccess,
	});

	return (
		<DisplayObject item={object}>
			<>
				<Button
					className="bg-gray-200 hover:bg-primary hover:text-white"
					loading={placeToKioskMutation.isLoading}
					onClick={() => placeToKioskMutation.mutate({ item: object, kioskId })}
				>
					Place in kiosk
				</Button>
				<Button
					className="border-gray-400 bg-transparent hover:bg-primary hover:text-white"
					onClick={() => listFn(object)}
				>
					List For Sale
				</Button>
			</>
		</DisplayObject>
	);
}
