// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import {
    WalletActionStake24,
    ArrowRight16,
    Info16,
    Swap16,
} from '@mysten/icons';
import cl from 'classnames';

import Icon, { SuiIcons } from '_components/icon';

// TODO: use update icons lib
const icons = {
    Send: (
        <ArrowRight16
            fill="currentColor"
            className="text-gradient-blue-start text-body -rotate-45"
        />
    ),
    Received: (
        <ArrowRight16
            fill="currentColor"
            className="text-gradient-blue-start text-body rotate-135"
        />
    ),
    Staked: (
        <WalletActionStake24 className="text-gradient-blue-start text-heading2 bg-transparent" />
    ),
    Unstaked: (
        <Icon
            icon={SuiIcons.Tokens}
            className="text-gradient-blue-start text-heading3"
        />
    ),
    Rewards: (
        <Icon
            icon={SuiIcons.SuiLogoIcon}
            className="text-gradient-blue-start text-body"
        />
    ),
    Swapped: <Swap16 className="text-gradient-blue-start text-heading6" />,
    Failed: <Info16 className="text-issue-dark text-heading6" />,
};

interface TxnItemIconProps {
    txnFailed?: boolean;
    variant:
        | 'Rewards'
        | 'Staked'
        | 'Unstaked'
        | 'Swapped'
        | 'Send'
        | 'Received';
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
