// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import { LockLocked16, LockUnlocked16 } from '@mysten/icons';
import { type ComponentPropsWithoutRef } from 'react';

import { Tooltip } from '../../shared/tooltip';
import LoadingIndicator from '../loading/LoadingIndicator';

interface LockUnlockButtonProps extends ComponentPropsWithoutRef<'button'> {
	isLocked: boolean;
	isLoading: boolean;
}

export function LockUnlockButton({ isLocked, onClick, isLoading }: LockUnlockButtonProps) {
	return (
		<Tooltip tip={isLocked ? 'Unlock Account' : 'Lock Account'}>
			<button
				className="appearance-none p-0 bg-transparent border-none cursor-pointer text-steel hover:text-hero-dark ml-auto flex items-center justify-center"
				onClick={onClick}
				data-testid={isLocked ? 'unlock-account-button' : 'lock-account-button'}
			>
				{isLoading ? (
					<LoadingIndicator />
				) : isLocked ? (
					<LockLocked16 className="h-4 w-4" />
				) : (
					<LockUnlocked16 className="h-4 w-4" />
				)}
			</button>
		</Tooltip>
	);
}
