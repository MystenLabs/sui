// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import { type SuiObjectResponse, getObjectDisplay, getObjectType } from '@mysten/sui.js';

import { useRecognizedPackages } from './useRecognizedPackages';

export function useResolveVideo(object: SuiObjectResponse) {
	const recognizedPackages = useRecognizedPackages();
	const objectType = getObjectType(object);
	const isRecognized = objectType && recognizedPackages.includes(objectType.split('::')[0]);

	if (!isRecognized) return null;

	const display = getObjectDisplay(object)?.data;

	return display?.video_url;
}
