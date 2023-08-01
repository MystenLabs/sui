// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import { LockLocked16, LockUnlocked16 } from '@mysten/icons';
import { type ComponentPropsWithoutRef } from 'react';
import { Tooltip } from '../../shared/tooltip';

interface LockUnlockButtonProps extends ComponentPropsWithoutRef<'button'> {
	isLocked: boolean;
}

export function LockUnlockButton({ isLocked, ...buttonProps }: LockUnlockButtonProps) {
	return (
		<Tooltip tip={isLocked ? 'Unlock Account' : 'Lock Account'}>
			<button
				className="appearance-none bg-transparent border-none cursor-pointer text-steel hover:text-hero-dark ml-auto flex items-center justify-center"
				{...buttonProps}
			>
				{isLocked ? <LockLocked16 className="h-4 w-4" /> : <LockUnlocked16 className="h-4 w-4" />}
			</button>
		</Tooltip>
	);
}
