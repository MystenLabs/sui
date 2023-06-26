// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import * as amplitude from '@amplitude/analytics-browser';
import { TransportType, type UserSession, LogLevel } from '@amplitude/analytics-types';
import { PersistableStorage } from '@mysten/core';

import { ampli } from './ampli';

const IS_PROD_ENV = process.env.NODE_ENV === 'production';

export const persistableStorage = new PersistableStorage<UserSession>();

export async function initAmplitude() {
	ampli.load({
		environment: IS_PROD_ENV ? 'production' : 'development',
		// Flip this if you'd like to test Amplitude locally
		disabled: !IS_PROD_ENV,
		client: {
			configuration: {
				cookieStorage: persistableStorage,
				logLevel: IS_PROD_ENV ? LogLevel.Warn : LogLevel.Debug,
			},
		},
	});

	window.addEventListener('pagehide', () => {
		amplitude.setTransport(TransportType.SendBeacon);
		amplitude.flush();
	});
}

export function getUrlWithDeviceId(url: URL) {
	const amplitudeDeviceId = ampli.client.getDeviceId();
	if (amplitudeDeviceId) {
		url.searchParams.append('deviceId', amplitudeDeviceId);
	}
	return url;
}
