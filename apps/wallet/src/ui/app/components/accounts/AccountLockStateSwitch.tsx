// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import { type ReactNode } from 'react';
import { type SerializedUIAccount } from '../../../../background/accounts/Account';
import { useIsAccountReadLocked } from '../../hooks/useIsAccountReadLocked';

export type AccountLockStateSwitchProps = {
	account: SerializedUIAccount | null;
	/** layout to show when the account is unlocked (write and read is allowed - executing transactions is possible) */
	fullyUnlockedLayout?: ReactNode;
	/** layout to show when the account is locked but read is allowed (was unlocked recently but for some reason the account keyPair was evicted from session storage) */
	readOnlyLayout?: ReactNode;
	/** layout to show when the account is unlocked at least for read (can be write unlocked as well)*/
	readLayout?: ReactNode;
	/** layout to show when the account is locked */
	writeLockedLayout?: ReactNode;
	/** layout to show when non of the provided states were applicable */
	elseLayout?: ReactNode;
};
export function AccountLockedStateSwitch({
	account,
	fullyUnlockedLayout,
	readOnlyLayout,
	readLayout,
	writeLockedLayout,
	elseLayout,
}: AccountLockStateSwitchProps) {
	const isWriteLocked = !account || account.isLocked;
	const isReadLocked = useIsAccountReadLocked(account);
	if (!account) {
		return null;
	}
	if (isWriteLocked && writeLockedLayout !== undefined) {
		return writeLockedLayout;
	}
	if (isWriteLocked && !isReadLocked && readOnlyLayout !== undefined) {
		return readOnlyLayout;
	}
	if (!isWriteLocked && !isReadLocked && fullyUnlockedLayout !== undefined) {
		return fullyUnlockedLayout;
	}
	if (!isReadLocked && readLayout !== undefined) {
		return readLayout;
	}
	return elseLayout || null;
}
