// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import cl from 'classnames';
import { memo, useCallback, useEffect, useState } from 'react';

import Icon, { SuiIcons } from '_components/icon';

import type { ReactNode, MouseEventHandler } from 'react';

import st from './CopyToClipboard.module.scss';

const COPY_CHECKMARK_MILLIS = 600;

export type CopyToClipboardProps = {
    txt: string;
    children: ReactNode;
    copyOnlyOnIconClick?: boolean;
    className?: string;
    mode?: 'normal' | 'highlighted' | 'plain';
};

function CopyToClipboard({
    txt,
    children,
    copyOnlyOnIconClick = false,
    className,
    mode = 'normal',
}: CopyToClipboardProps) {
    const [copied, setCopied] = useState(false);
    const copyToClipboard = useCallback<MouseEventHandler<HTMLElement>>(
        async (e) => {
            e.stopPropagation();
            e.preventDefault();
            if (!txt) {
                return;
            }
            await navigator.clipboard.writeText(txt);
            setCopied(true);
        },
        [txt]
    );
    useEffect(() => {
        let timeout: number;
        if (copied) {
            timeout = window.setTimeout(
                () => setCopied(false),
                COPY_CHECKMARK_MILLIS
            );
        }
        return () => {
            if (timeout) {
                clearTimeout(timeout);
            }
        };
    }, [copied]);
    return (
        <span
            className={cl(st.container, className)}
            onClick={!copyOnlyOnIconClick ? copyToClipboard : undefined}
        >
            {children}
            <Icon
                className={cl(st.copyIcon, st[mode], { [st.copied]: copied })}
                icon={SuiIcons.Copy}
                onClick={copyToClipboard}
                title="Copy to clipboard"
            />
        </span>
    );
}

export default memo(CopyToClipboard);
