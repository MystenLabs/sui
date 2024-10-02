// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import { useContext } from 'react';

import { KioskClientContext } from '../components/KioskClientProvider';

export function useKioskClient() {
	const kioskClient = useContext(KioskClientContext);
	if (!kioskClient) {
		throw new Error('Kiosk client not found. Please make sure KioskClientProvider is set up.');
	}
	return kioskClient;
}
