// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0
import { SocialGoogle24, SocialTwitch24, Sui } from '@mysten/icons';
import { type SerializedUIAccount } from '_src/background/accounts/Account';
import { isZkAccountSerializedUI } from '_src/background/accounts/zk/ZkAccount';

function SuiIcon() {
	return (
		<div className="bg-sui-primaryBlue2023 rounded-full text-white h-4 w-4 flex items-center justify-center p-1">
			<Sui />
		</div>
	);
}

function ProviderIcon({ provider }: { provider: string }) {
	switch (provider) {
		case 'google':
			return <SocialGoogle24 />;
		case 'twitch':
			return <SocialTwitch24 />;
		default:
			// default to Sui for now
			return <SuiIcon />;
	}
}

export function AccountIcon({ account }: { account: SerializedUIAccount }) {
	if (isZkAccountSerializedUI(account)) {
		return <ProviderIcon provider={account.provider} />;
	}
	return <SuiIcon />;
}
