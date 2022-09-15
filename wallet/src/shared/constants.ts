// Copyright (c) 2022, Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import Plausible from 'plausible-tracker';
const WALLET_URL = 'chrome-extension://opcgpfmipidbgpenhmajoajpbobppdil';

export const ToS_LINK = 'https://sui.io/terms';
export const PRIVACY_POLICY_LINK = 'https://sui.io/policy/';
export const { trackPageview, trackEvent, enableAutoPageviews } = Plausible({
    domain: WALLET_URL,
});