// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

export type ZkProvider = 'Google';

export interface ZkProviderData {
	clientID: string;
	url: string;
}

export const zkProviderDataMap: Record<ZkProvider, ZkProviderData> = {
	Google: {
		clientID: '946731352276-pk5glcg8cqo38ndb39h7j093fpsphusu.apps.googleusercontent.com',
		url: 'https://accounts.google.com/o/oauth2/v2/auth',
	},
};
