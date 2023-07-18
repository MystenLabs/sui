// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import {
	SuiObjectData,
	SuiObjectResponse,
	getSuiObjectData,
	isSuiObjectResponse,
} from '@mysten/sui.js';
import { KIOSK_OWNER_CAP } from '@mysten/kiosk';

export const ORIGINBYTE_KIOSK_MODULE =
	'0x95a441d389b07437d00dd07e0b6f05f513d7659b13fd7c5d3923c7d9d847199b::ob_kiosk';

export const ORIGINBYTE_KIOSK_OWNER_TOKEN = `${ORIGINBYTE_KIOSK_MODULE}::OwnerToken`;

export function isKioskOwnerToken(object?: SuiObjectResponse | SuiObjectData) {
	if (!object) return false;
	const objectData = isSuiObjectResponse(object) ? getSuiObjectData(object) : object;
	return [KIOSK_OWNER_CAP, ORIGINBYTE_KIOSK_OWNER_TOKEN].includes(objectData?.type ?? '');
}

export function getKioskIdFromDynamicFields(object: SuiObjectResponse | SuiObjectData) {
	const objectData = isSuiObjectResponse(object) ? getSuiObjectData(object) : object;
	return (
		objectData?.content &&
		'fields' in objectData.content &&
		(objectData.content.fields.for ?? objectData.content.fields.kiosk)
	);
}
