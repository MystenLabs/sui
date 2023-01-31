// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import cl from 'classnames';

import Icon, { SuiIcons } from '_components/icon';

export function StatusIcon({ status }: { status: boolean }) {
    return (
        <div
            className={cl(
                'rounded-full w-12 h-12 border-dotted  border-2 flex items-center justify-center mb-2.5 p-1',
                status ? 'border-success' : 'border-issue'
            )}
        >
            <div
                className={cl(
                    'bg-success rounded-full h-8 w-8 flex items-center justify-center',
                    status ? 'border-success' : 'border-issue'
                )}
            >
                <Icon
                    icon={SuiIcons.ThumbsUp}
                    className={cl(
                        'text-white text-2xl',
                        !status && 'rotate-180'
                    )}
                />
            </div>
        </div>
    );
}
