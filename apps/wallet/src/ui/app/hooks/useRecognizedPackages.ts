// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import { API_ENV } from '_src/shared/api-env';
import { useFeatureValue } from '@growthbook/growthbook-react';
import { SUI_FRAMEWORK_ADDRESS, SUI_SYSTEM_ADDRESS } from '@mysten/sui/utils';

import useAppSelector from './useAppSelector';

const DEFAULT_RECOGNIZED_PACKAGES = [SUI_FRAMEWORK_ADDRESS, SUI_SYSTEM_ADDRESS];

export function useRecognizedPackages() {
	const apiEnv = useAppSelector((app) => app.app.apiEnv);
	const recognizedPackages = useFeatureValue('recognized-packages', DEFAULT_RECOGNIZED_PACKAGES);

	// Our recognized package list is currently only available on mainnet
	return apiEnv === API_ENV.mainnet ? recognizedPackages : DEFAULT_RECOGNIZED_PACKAGES;
}
