// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import { type SerializedUIAccount } from '_src/background/accounts/Account';
import { isZkLoginAccountSerializedUI } from '_src/background/accounts/zklogin/ZkLoginAccount';

import { Button } from '../../shared/ButtonUI';
import { SocialButton } from '../../shared/SocialButton';
import { useUnlockAccount } from './UnlockAccountContext';

export type UnlockAccountButtonProps = {
	account: SerializedUIAccount;
	title?: string;
};
export function UnlockAccountButton({
	account,
	title = 'Unlock Account',
}: UnlockAccountButtonProps) {
	const { isPasswordUnlockable } = account;
	const { unlockAccount, isPending } = useUnlockAccount();

	if (isPasswordUnlockable) {
		return <Button text={title} onClick={() => unlockAccount(account)} />;
	}
	if (isZkLoginAccountSerializedUI(account)) {
		return (
			<SocialButton
				provider={account.provider}
				onClick={() => {
					unlockAccount(account);
				}}
				loading={isPending}
				showLabel
			/>
		);
	}
}
