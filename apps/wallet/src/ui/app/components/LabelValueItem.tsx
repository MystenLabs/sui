// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import cl from 'classnames';
import { type ReactNode } from 'react';

import { Link } from '_src/ui/app/shared/Link';
import { Text } from '_src/ui/app/shared/text';

function getDisplayUrl(link: string) {
    try {
        const url = new URL(link);
        return { href: link, display: url.hostname };
    } catch (e) {
        return link || null;
    }
}

export type LabelValueItemProps = {
    label: string;
    value: ReactNode;
    multiline?: boolean;
    parseUrl?: boolean;
};

export function LabelValueItem({
    label,
    value,
    multiline = false,
    parseUrl = false,
}: LabelValueItemProps) {
    let href: string | null = null;
    let display: ReactNode | null = null;
    if (parseUrl && typeof value === 'string') {
        const displayUrl = getDisplayUrl(value);
        if (typeof displayUrl === 'string') {
            display = displayUrl;
        } else if (displayUrl) {
            href = displayUrl.href;
            display = displayUrl.display;
        }
    } else if (
        typeof value === 'string' &&
        (value.startsWith('http://') || value.startsWith('https://'))
    ) {
        href = display = value;
    } else {
        display = value;
    }
    return display ? (
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
    ) : null;
}
