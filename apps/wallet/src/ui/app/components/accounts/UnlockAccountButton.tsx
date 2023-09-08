// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import { useUnlockAccount } from './UnlockAccountContext';

import { Button } from '../../shared/ButtonUI';
import { SocialButton } from '../../shared/SocialButton';
import { type SerializedUIAccount } from '_src/background/accounts/Account';
import { isZkAccountSerializedUI } from '_src/background/accounts/zk/ZkAccount';

export type UnlockAccountButtonProps = {
	account: SerializedUIAccount;
	title?: string;
};
export function UnlockAccountButton({
	account,
	title = 'Unlock Account',
}: UnlockAccountButtonProps) {
	const { isPasswordUnlockable } = account;
	const { unlockAccount, isLoading } = useUnlockAccount();

	if (isPasswordUnlockable) {
		return <Button text={title} onClick={() => unlockAccount(account)} />;
	}
	if (isZkAccountSerializedUI(account)) {
		return (
			<SocialButton
				provider={account.provider}
				onClick={() => {
					unlockAccount(account);
				}}
				loading={isLoading}
				showLabel
			/>
		);
	}
}
