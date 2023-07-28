// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import { SuiObjectData, SuiObjectResponse } from '@mysten/sui.js/client';
import { KIOSK_OWNER_CAP } from '@mysten/kiosk';

export const ORIGINBYTE_KIOSK_MODULE =
	'0x95a441d389b07437d00dd07e0b6f05f513d7659b13fd7c5d3923c7d9d847199b::ob_kiosk';

export const ORIGINBYTE_KIOSK_OWNER_TOKEN = `${ORIGINBYTE_KIOSK_MODULE}::OwnerToken`;

export function isKioskOwnerToken(object?: SuiObjectResponse | SuiObjectData | null) {
	if (!object) return false;
	const objectData = 'data' in object && object.data ? object.data : (object as SuiObjectData);
	return [KIOSK_OWNER_CAP, ORIGINBYTE_KIOSK_OWNER_TOKEN].includes(objectData?.type ?? '');
}

export function getKioskIdFromOwnerCap(object: SuiObjectResponse | SuiObjectData) {
	const objectData = 'data' in object && object.data ? object.data : (object as SuiObjectData);
	const fields =
		objectData.content?.dataType === 'moveObject'
			? (objectData.content.fields as { for?: string; kiosk?: string })
			: null;
	return fields?.for ?? fields?.kiosk!;
}
