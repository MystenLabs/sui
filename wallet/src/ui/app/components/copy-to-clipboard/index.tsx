// Copyright (c) 2022, Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import cl from 'classnames';
import { memo, useCallback, useEffect, useState } from 'react';

import BsIcon from '_components/bs-icon';

import type { ReactNode, MouseEventHandler } from 'react';

import st from './CopyToClipboard.module.scss';

const COPY_CHECKMARK_MILLIS = 600;

export type CopyToClipboardProps = {
    txt: string;
    children: ReactNode;
    copyOnlyOnIconClick?: boolean;
    className?: string;
};

function CopyToClipboard({
    txt,
    children,
    copyOnlyOnIconClick = false,
    className,
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
            <BsIcon
                className={st['copy-icon']}
                icon={`clipboard${copied ? '-check' : ''}`}
                onClick={copyToClipboard}
                title="Copy to clipboard"
            />
        </span>
    );
}

export default memo(CopyToClipboard);
