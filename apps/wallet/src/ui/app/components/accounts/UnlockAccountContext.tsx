// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import { type SerializedUIAccount } from '_src/background/accounts/Account';
import React, { createContext, useCallback, useContext, useState, type ReactNode } from 'react';
import { toast } from 'react-hot-toast';

import { useBackgroundClient } from '../../hooks/useBackgroundClient';
import { useUnlockMutation } from '../../hooks/useUnlockMutation';
import { UnlockAccountModal } from './UnlockAccountModal';

interface UnlockAccountContextType {
	isUnlockModalOpen: boolean;
	accountToUnlock: SerializedUIAccount | null;
	unlockAccount: (account: SerializedUIAccount) => void;
	lockAccount: (account: SerializedUIAccount) => void;
	isPending: boolean;
	hideUnlockModal: () => void;
}

const UnlockAccountContext = createContext<UnlockAccountContextType | null>(null);

export const UnlockAccountProvider = ({ children }: { children: ReactNode }) => {
	const [isUnlockModalOpen, setIsUnlockModalOpen] = useState(false);
	const [accountToUnlock, setAccountToUnlock] = useState<SerializedUIAccount | null>(null);
	const unlockAccountMutation = useUnlockMutation();
	const backgroundClient = useBackgroundClient();
	const hideUnlockModal = useCallback(() => {
		setIsUnlockModalOpen(false);
		setAccountToUnlock(null);
	}, []);

	const unlockAccount = useCallback(
		async (account: SerializedUIAccount) => {
			if (account) {
				if (account.isPasswordUnlockable) {
					// for password-unlockable accounts, show the unlock modal
					setIsUnlockModalOpen(true);
					setAccountToUnlock(account);
				} else {
					try {
						// for non-password-unlockable accounts, unlock directly
						setAccountToUnlock(account);
						await unlockAccountMutation.mutateAsync({ id: account.id });
						setAccountToUnlock(null);
						toast.success('Account unlocked');
					} catch (e) {
						toast.error((e as Error).message || 'Failed to unlock account');
					}
				}
			}
		},
		[unlockAccountMutation],
	);

	const lockAccount = useCallback(
		async (account: SerializedUIAccount) => {
			try {
				await backgroundClient.lockAccountSourceOrAccount({ id: account.id });
				toast.success('Account locked');
			} catch (e) {
				toast.error((e as Error).message || 'Failed to lock account');
			}
		},
		[backgroundClient],
	);

	return (
		<UnlockAccountContext.Provider
			value={{
				isUnlockModalOpen,
				accountToUnlock,
				unlockAccount,
				hideUnlockModal,
				lockAccount,
				isPending: unlockAccountMutation.isPending,
			}}
		>
			{children}
			<UnlockAccountModal
				onClose={hideUnlockModal}
				onSuccess={hideUnlockModal}
				account={accountToUnlock}
				open={isUnlockModalOpen}
			/>
		</UnlockAccountContext.Provider>
	);
};

export const useUnlockAccount = (): UnlockAccountContextType => {
	const context = useContext(UnlockAccountContext);
	if (!context) {
		throw new Error('useUnlockAccount must be used within an UnlockAccountProvider');
	}
	return context;
};
