// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import {
	Account24,
	ArrowRight16,
	Info16,
	Sui,
	Swap16,
	Unstaked,
	WalletActionStake24,
} from '@mysten/icons';
import cl from 'clsx';

import LoadingIndicator from '../loading/LoadingIndicator';

const icons = {
	Send: (
		<ArrowRight16 fill="currentColor" className="text-gradient-blue-start text-body -rotate-45" />
	),
	Receive: (
		<ArrowRight16 fill="currentColor" className="text-gradient-blue-start text-body rotate-135" />
	),
	Transaction: (
		<ArrowRight16 fill="currentColor" className="text-gradient-blue-start text-body -rotate-45" />
	),
	Staked: <WalletActionStake24 className="text-gradient-blue-start text-heading2 bg-transparent" />,
	Unstaked: <Unstaked className="text-gradient-blue-start text-heading3" />,
	Rewards: <Sui className="text-gradient-blue-start text-body" />,
	Swapped: <Swap16 className="text-gradient-blue-start text-heading6" />,
	Failed: <Info16 className="text-issue-dark text-heading6" />,
	Loading: <LoadingIndicator />,
	PersonalMessage: <Account24 fill="currentColor" className="text-gradient-blue-start text-body" />,
};

interface TxnItemIconProps {
	txnFailed?: boolean;
	variant: keyof typeof icons;
}

export function TxnIcon({ txnFailed, variant }: TxnItemIconProps) {
	return (
		<div
			className={cl([
				txnFailed ? 'bg-issue-light' : 'bg-gray-40',
				'w-7.5 h-7.5 flex justify-center items-center rounded-2lg',
			])}
		>
			{icons[txnFailed ? 'Failed' : variant]}
		</div>
	);
}
