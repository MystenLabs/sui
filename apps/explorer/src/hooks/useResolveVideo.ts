// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import { getObjectDisplay, getObjectType } from '@mysten/sui.js';
import { type SuiObjectResponse } from '@mysten/sui.js/client';

import { useRecognizedPackages } from './useRecognizedPackages';

export function useResolveVideo(object: SuiObjectResponse) {
	const recognizedPackages = useRecognizedPackages();
	const objectType = getObjectType(object);
	const isRecognized = objectType && recognizedPackages.includes(objectType.split('::')[0]);

	if (!isRecognized) return null;

	const display = getObjectDisplay(object)?.data;

	return display?.video_url;
}
