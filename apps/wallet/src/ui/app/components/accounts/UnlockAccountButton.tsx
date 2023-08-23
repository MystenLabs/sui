// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import { useState } from 'react';
import { UnlockAccountModal } from './UnlockAccountModal';
import { useUnlockMutation } from '../../hooks/useUnlockMutation';
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
	const { id, isPasswordUnlockable } = account;
	const unlockMutation = useUnlockMutation();
	const [isPasswordModalVisible, setIsPasswordModalVisible] = useState(false);
	if (isPasswordModalVisible) {
		return (
			<UnlockAccountModal
				onClose={() => setIsPasswordModalVisible(false)}
				onSuccess={(password: string) => unlockMutation.mutateAsync({ id, password })}
			/>
		);
	}
	if (isPasswordUnlockable) {
		return (
			<Button
				text={title}
				onClick={() => setIsPasswordModalVisible(true)}
				disabled={isPasswordModalVisible}
			/>
		);
	}
	if (isZkAccountSerializedUI(account)) {
		return (
			<SocialButton
				provider={account.provider}
				onClick={() => {
					unlockMutation.mutate({ id });
				}}
				loading={unlockMutation.isLoading}
				showLabel
			/>
		);
	}
}
