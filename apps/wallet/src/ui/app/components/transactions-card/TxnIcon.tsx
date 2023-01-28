// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import cl from 'classnames';

import Icon, { SuiIcons } from '_components/icon';

// TODO: use update icons lib
const icons = {
    Send: (
        <Icon
            icon={SuiIcons.ArrowLeft}
            className="text-gradient-blue-start rotate-135"
        />
    ),
    Received: (
        <Icon
            icon={SuiIcons.ArrowLeft}
            className="text-gradient-blue-start -rotate-45"
        />
    ),
    Staked: <Icon icon={SuiIcons.Union} className="text-gradient-blue-start" />,
    UnStaked: (
        <Icon icon={SuiIcons.Tokens} className="text-gradient-blue-start" />
    ),
    Rewards: (
        <Icon
            icon={SuiIcons.SuiLogoIcon}
            className="text-gradient-blue-start text-body"
        />
    ),
    Swapped: <Icon icon={SuiIcons.Swap} className="text-gradient-blue-start" />,
    Failed: <Icon icon={SuiIcons.Info} className="text-issue-dark text-body" />,
};

interface TxnItemIconProps {
    txnFailed?: boolean;
    variant:
        | 'Rewards'
        | 'Staked'
        | 'UnStaked'
        | 'Swapped'
        | 'Send'
        | 'Received';
}

export function TxnIcon({ txnFailed, variant }: TxnItemIconProps) {
    return (
        <div
            className={cl([
                txnFailed ? 'bg-issue-light' : 'bg-gray-45',
                'w-7.5 h-7.5 flex justify-center items-center rounded-2lg',
            ])}
        >
            {icons[txnFailed ? 'Failed' : variant]}
        </div>
    );
}
