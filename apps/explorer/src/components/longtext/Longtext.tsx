// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import { useCallback, useState, useContext } from 'react';
import toast from 'react-hot-toast';

import { ReactComponent as ContentArrowRight } from '../../assets/SVGIcons/12px/ArrowRight.svg';
import { ReactComponent as ContentCopyIcon16 } from '../../assets/SVGIcons/16px/Copy.svg';
import { ReactComponent as ContentCopyIcon24 } from '../../assets/SVGIcons/24px/Copy.svg';
import { NetworkContext } from '../../context';

import type { ReactNode } from 'react';

import styles from './Longtext.module.css';

import { Link } from '~/ui/Link';
import { useNavigateWithQuery } from '~/ui/utils/LinkWithQuery';
import { navigateWithUnknown } from '~/utils/api/searchUtil';

/** @deprecated Use new UI components instead of Longtext. */
function Longtext({
    text,
    category = 'unknown',
    isLink = true,
    alttext = '',
    copyButton = 'none',
    showIconButton = false,
    extra,
}: {
    text: string;
    category: 'object' | 'transaction' | 'address' | 'validators' | 'unknown';
    isLink?: boolean;
    alttext?: string;
    copyButton?: '16' | '24' | 'none';
    showIconButton?: boolean;
    extra?: ReactNode;
}) {
    const [pleaseWait, setPleaseWait] = useState(false);
    const [network] = useContext(NetworkContext);
    const navigate = useNavigateWithQuery();

    const handleCopyEvent = useCallback(() => {
        navigator.clipboard.writeText(text);
        toast.success('Copied!');
    }, [text]);

    let icon = null;
    let iconButton = null;

    if (copyButton !== 'none') {
        if (pleaseWait) {
            icon = <div className={styles.copied}>&#8987; Please Wait</div>;
        } else {
            icon = (
                <div
                    className={
                        copyButton === '16' ? styles.copy16 : styles.copy24
                    }
                    onClick={handleCopyEvent}
                >
                    {copyButton === '16' ? (
                        <ContentCopyIcon16 />
                    ) : (
                        <ContentCopyIcon24 />
                    )}
                </div>
            );
        }
    }

    if (showIconButton) {
        iconButton = <ContentArrowRight />;
    }

    const navigateUnknown = useCallback(() => {
        setPleaseWait(true);
        navigateWithUnknown(text, navigate, network).then(() =>
            setPleaseWait(false)
        );
    }, [text, navigate, network]);

    // temporary hack to make display of the genesis transaction clearer
    if (
        category === 'transaction' &&
        text === 'AAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAA='
    ) {
        text = 'Genesis';
        isLink = false;
    }

    let textComponent;
    if (isLink) {
        if (category === 'unknown') {
            textComponent = (
                <Link variant="mono" onClick={navigateUnknown}>
                    {alttext ? alttext : text}
                </Link>
            );
        } else {
            textComponent = (
                <Link
                    variant="mono"
                    to={`/${category}/${encodeURIComponent(text)}`}
                >
                    {alttext ? alttext : text} {iconButton}
                </Link>
            );
        }
    } else {
        textComponent = (
            <div className="text-sui-grey-90 font-mono font-medium">
                {alttext ? alttext : text}
            </div>
        );
    }

    return (
        <div className={styles.longtextwrapper}>
            {textComponent}
            {extra ? <div className={styles.extra}>{extra}</div> : null}
            {icon}
        </div>
    );
}

export default Longtext;
