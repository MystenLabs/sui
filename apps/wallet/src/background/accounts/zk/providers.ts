// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

export type ZkProvider = 'google' | 'twitch' | 'facebook' | 'microsoft';

export interface ZkProviderData {
	clientID: string;
	url: string;
}

export const zkProviderDataMap: Record<ZkProvider, ZkProviderData> = {
	google: {
		clientID: '946731352276-pk5glcg8cqo38ndb39h7j093fpsphusu.apps.googleusercontent.com',
		url: 'https://accounts.google.com/o/oauth2/v2/auth',
	},
	// TODO: update this before enabling them
	twitch: { clientID: '', url: '' },
	facebook: { clientID: '', url: '' },
	microsoft: { clientID: '', url: '' },
};
