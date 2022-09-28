// Copyright (c) 2022, Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import Plausible from 'plausible-tracker';

export const ToS_LINK = 'https://sui.io/terms';
export const PRIVACY_POLICY_LINK = 'https://sui.io/policy/';
// NOTE: The url of Sui wallet Chrome extension:
// https://chrome.google.com/webstore/detail/sui-wallet/opcgpfmipidbgpenhmajoajpbobppdil
export const WALLET_URL = 'chrome-extension://opcgpfmipidbgpenhmajoajpbobppdil';
export const plausible = Plausible({
    domain: WALLET_URL,
});
