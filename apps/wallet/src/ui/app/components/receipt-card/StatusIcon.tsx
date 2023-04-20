// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import { ThumbUpFill32 } from '@mysten/icons';
import cl from 'classnames';

export function StatusIcon({ status }: { status: boolean }) {
    return (
        <div
            className={cl(
                'rounded-full w-12 h-12 border-dotted  border-2 flex items-center justify-center p-1',
                status ? 'border-success' : 'border-issue'
            )}
        >
            <div
                className={cl(
                    'rounded-full h-8 w-8 flex items-center justify-center',
                    status ? 'bg-success' : 'bg-issue'
                )}
            >
                <ThumbUpFill32
                    fill="currentColor"
                    className={cl(
                        'text-white text-2xl',
                        !status && 'rotate-180'
                    )}
                />
            </div>
        </div>
    );
}
