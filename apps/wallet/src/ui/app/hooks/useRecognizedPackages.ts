// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import { SUI_FRAMEWORK_ADDRESS } from '@mysten/sui.js';

const DEFAULT_RECOGNIZED_PACKAGES = [SUI_FRAMEWORK_ADDRESS];

export function useRecognizedPackages() {
    return DEFAULT_RECOGNIZED_PACKAGES;
}
