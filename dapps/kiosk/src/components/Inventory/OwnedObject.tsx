// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import { KioskFnType } from '../../hooks/kiosk';
import { usePlaceMutation } from '../../mutations/kiosk';
import { Button } from '../Base/Button';
import { DisplayObject } from '../DisplayObject';
import { OwnedObjectType } from './OwnedObjects';

export function OwnedObject({
	object,
	onListSuccess,
	listFn,
	kioskId,
}: {
	onListSuccess: () => void;
	listFn: KioskFnType;
	object: OwnedObjectType;
	kioskId: string;
}) {
	const placeToKioskMutation = usePlaceMutation({
		onSuccess: onListSuccess,
	});

	return (
		<DisplayObject item={object}>
			<>
				<Button
					className="bg-gray-200 hover:bg-primary hover:text-white"
					loading={placeToKioskMutation.isPending}
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
