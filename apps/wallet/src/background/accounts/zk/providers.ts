// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

export type ZkProvider = 'google' | 'twitch' | 'facebook';

export interface ZkProviderData {
	clientID: string;
	url: string;
	extraParams?: Record<string, string>;
	buildExtraParams?: (inputs: {
		prompt?: boolean;
		loginHint?: string;
		params: URLSearchParams;
	}) => void;
	enabled: boolean;
}

export const zkProviderDataMap: Record<ZkProvider, ZkProviderData> = {
	google: {
		clientID: '946731352276-pk5glcg8cqo38ndb39h7j093fpsphusu.apps.googleusercontent.com',
		url: 'https://accounts.google.com/o/oauth2/v2/auth',
		extraParams: {
			response_type: 'id_token',
			scope: 'openid email profile',
		},
		buildExtraParams: ({ prompt, loginHint, params }) => {
			if (prompt) {
				params.append('prompt', 'select_account');
			}
			if (loginHint) {
				params.append('login_hint', loginHint);
			}
		},
		enabled: true,
	},
	twitch: {
		clientID: 'uzpfot3uotf7fp9hklsyctn2735bcw',
		url: 'https://id.twitch.tv/oauth2/authorize',
		extraParams: {
			// adding token in the response_type allows the silent auth to work - without it, every time the auth window is shown
			response_type: 'token id_token',
			scope: 'openid user:read:email',
			claims: JSON.stringify({
				id_token: {
					email: null,
					email_verified: null,
					picture: null,
				},
			}),
		},
		buildExtraParams: ({ prompt, params }) => {
			if (prompt) {
				params.append('force_verify', 'true');
			}
		},
		enabled: true,
	},
	facebook: {
		clientID: '1544965116313994',
		url: 'https://facebook.com/dialog/oauth/',
		extraParams: {
			response_type: 'id_token',
			scope: 'openid email',
		},
		enabled: true,
	},
};
