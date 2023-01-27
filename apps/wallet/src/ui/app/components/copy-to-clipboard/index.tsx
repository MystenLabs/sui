// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import cl from 'classnames';

import { useCopyToClipboard } from '../../hooks/useCopyToClipboard';
import Icon, { SuiIcons } from '_components/icon';

import type { ReactNode } from 'react';

import st from './CopyToClipboard.module.scss';

export type CopyToClipboardProps = {
    txt: string;
    children: ReactNode;
    copyOnlyOnIconClick?: boolean;
    className?: string;
    mode?: 'normal' | 'highlighted' | 'plain';
    copySuccessMessage?: string;
};

function CopyToClipboard({
    txt,
    children,
    copyOnlyOnIconClick = false,
    className,
    mode = 'normal',
    copySuccessMessage,
}: CopyToClipboardProps) {
    const copyToClipboard = useCopyToClipboard(txt, { copySuccessMessage });
    return (
        <span
            className={cl(st.container, className)}
            onClick={!copyOnlyOnIconClick ? copyToClipboard : undefined}
        >
            {children}
            <Icon
                className={cl(st.copyIcon, st[mode])}
                icon={SuiIcons.Copy}
                onClick={copyToClipboard}
                title="Copy to clipboard"
            />
        </span>
    );
}

export default CopyToClipboard;
