// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import Plausible from 'plausible-tracker';

export const plausible = Plausible({
	// NOTE: If you want to test the plausible integration, you can uncomment
	// the following two lines which will start emitting events to plausible.
	// domain: 'explorer.ci.sui.io',
	// trackLocalhost: true,
});
