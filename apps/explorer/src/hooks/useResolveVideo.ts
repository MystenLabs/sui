// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import { type SuiObjectResponse } from '@mysten/sui.js/client';

import { useRecognizedPackages } from './useRecognizedPackages';

export function useResolveVideo(object: SuiObjectResponse) {
	const recognizedPackages = useRecognizedPackages();
	const objectType =
		object.data?.type ?? object?.data?.content?.dataType === 'package'
			? 'package'
			: object?.data?.content?.type;
	const isRecognized = objectType && recognizedPackages.includes(objectType.split('::')[0]);

	if (!isRecognized) return null;

	const display = object.data?.display?.data;

	return display?.video_url;
}
