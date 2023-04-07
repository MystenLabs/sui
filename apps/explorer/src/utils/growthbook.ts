// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import { GrowthBook } from '@growthbook/growthbook';

const GROWTHBOOK_API_KEY = import.meta.env.PROD
    ? 'sdk-fHnfPId19IG3Lhj'
    : 'sdk-qEEo0utCXJO2Oid3';

export const growthbook = new GrowthBook({
    apiHost: 'https://cdn.growthbook.io',
    clientKey: GROWTHBOOK_API_KEY,
    enableDevMode: true,
    // trackingCallback: (experiment, result) => {
    //     // TODO: Use your real analytics tracking system
    //     console.log('Viewed Experiment', {
    //         experimentId: experiment.key,
    //         variationId: result.key,
    //     });
    // },
});

export enum GROWTHBOOK_FEATURES {
    USE_TEST_NET_ENDPOINT = 'testnet-selection',
    VALIDATOR_PAGE_STAKING = 'validator-page-staking',
}
