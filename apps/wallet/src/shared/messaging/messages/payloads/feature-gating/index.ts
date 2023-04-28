// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import { isBasePayload } from '_payloads';

import type { GrowthBook } from '@growthbook/growthbook';
import type { BasePayload, Payload } from '_payloads';

export type LoadedFeatures = Parameters<GrowthBook['setFeatures']>['0'];
export type LoadedAttributes = Parameters<GrowthBook['setAttributes']>['0'];

export interface LoadedFeaturesPayload extends BasePayload {
    type: 'features-response';
    features: LoadedFeatures;
    attributes: LoadedAttributes;
}

export function isLoadedFeaturesPayload(
    payload: Payload
): payload is LoadedFeaturesPayload {
    return isBasePayload(payload) && payload.type === 'features-response';
}
