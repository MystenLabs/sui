// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import cl from 'classnames';
import { type ReactNode } from 'react';

import { type LinkData } from '_src/ui/app/hooks/useGetNFTMeta';
import { Link } from '_src/ui/app/shared/Link';
import { Text } from '_src/ui/app/shared/text';

export type LabelValueItemProps = {
    label: string;
    value: ReactNode | LinkData;
    multiline?: boolean;
};

export function LabelValueItem({
    label,
    value,
    multiline = false,
}: LabelValueItemProps) {
    let href: string | null = null;
    let display: ReactNode | null = null;
    if (typeof value === 'object' && value && 'href' in value) {
        href = value.href;
        display = value.display;
    } else if (
        typeof value === 'string' &&
        (value.startsWith('http://') || value.startsWith('https://'))
    ) {
        href = display = value;
    } else {
        display = value;
    }
    return (
        <div className="flex flex-row flex-nowrap gap-1">
            <div className="flex-1 overflow-hidden">
                <Text
                    color="steel-dark"
                    variant="body"
                    weight="medium"
                    truncate
                >
                    {label}
                </Text>
            </div>
            <div
                className={cl('max-w-[60%] break-words text-end', {
                    'pr-px line-clamp-3 hover:line-clamp-none': multiline,
                })}
            >
                {href && display ? (
                    <Link
                        color="suiDark"
                        weight="medium"
                        size="body"
                        href={href}
                        text={display}
                    />
                ) : (
                    <Text
                        color="steel-darker"
                        weight="medium"
                        truncate={!multiline}
                    >
                        {display}
                    </Text>
                )}
            </div>
        </div>
    );
}
