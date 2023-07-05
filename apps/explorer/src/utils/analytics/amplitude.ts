// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import * as amplitude from '@amplitude/analytics-browser';
import { LogLevel, TransportType, type UserSession } from '@amplitude/analytics-types';
import { PersistableStorage } from '@mysten/core';

import { ampli } from './ampli';

const IS_PROD_ENV = import.meta.env.PROD;

export const persistableStorage = new PersistableStorage<UserSession>();

export async function initAmplitude() {
	ampli.load({
		environment: IS_PROD_ENV ? 'production' : 'development',
		// Flip this if you'd like to test Amplitude locally
		disabled: !IS_PROD_ENV,
		client: {
			configuration: {
				cookieStorage: persistableStorage,
				logLevel: IS_PROD_ENV ? LogLevel.Warn : amplitude.Types.LogLevel.Debug,
			},
		},
	});

	window.addEventListener('pagehide', () => {
		amplitude.setTransport(TransportType.SendBeacon);
		amplitude.flush();
	});
}
