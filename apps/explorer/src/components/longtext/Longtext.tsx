// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import { useCallback, useState, useContext } from 'react';
import { Link, useNavigate } from 'react-router-dom';

import { ReactComponent as ContentArrowRight } from '../../assets/SVGIcons/12px/ArrowRight.svg';
import { ReactComponent as ContentCopyIcon16 } from '../../assets/SVGIcons/16px/Copy.svg';
import { ReactComponent as ContentCopyIcon24 } from '../../assets/SVGIcons/24px/Copy.svg';
import { NetworkContext } from '../../context';
import { navigateWithUnknown } from '../../utils/searchUtil';
import ExternalLink from '../external-link/ExternalLink';

import styles from './Longtext.module.css';

function Longtext({
    text,
    category = 'unknown',
    isLink = true,
    alttext = '',
    copyButton = 'none',
    showIconButton = false,
}: {
    text: string;
    category:
        | 'objects'
        | 'transactions'
        | 'addresses'
        | 'ethAddress'
        | 'validators'
        | 'unknown';
    isLink?: boolean;
    alttext?: string;
    copyButton?: '16' | '24' | 'none';
    showIconButton?: boolean;
}) {
    const [isCopyIcon, setCopyIcon] = useState(true);

    const [pleaseWait, setPleaseWait] = useState(false);
    const [network] = useContext(NetworkContext);
    const navigate = useNavigate();

    const handleCopyEvent = useCallback(() => {
        navigator.clipboard.writeText(text);
        setCopyIcon(false);
        setTimeout(() => setCopyIcon(true), 1000);
    }, [setCopyIcon, text]);

    let icon;
    let iconButton = <></>;

    if (copyButton !== 'none') {
        if (pleaseWait) {
            icon = <div className={styles.copied}>&#8987; Please Wait</div>;
        } else if (isCopyIcon) {
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
        } else {
            icon = (
                <span className={styles.copied}>
                    <span>&#10003;</span> <span>Copied</span>
                </span>
            );
        }
    } else {
        icon = <></>;
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
        category === 'transactions' &&
        text === 'AAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAA='
    ) {
        text = 'Genesis';
        isLink = false;
    }

    let textComponent;
    if (isLink) {
        if (category === 'unknown') {
            textComponent = (
                <div className={styles.longtext} onClick={navigateUnknown}>
                    {alttext ? alttext : text}
                </div>
            );
        } else if (category === 'ethAddress') {
            textComponent = (
                <ExternalLink
                    href={`https://etherscan.io/address/${text}`}
                    label={text}
                    className={styles.longtext}
                />
            );
        } else {
            textComponent = (
                <div>
                    <Link
                        className={styles.longtext}
                        to={`/${category}/${encodeURIComponent(text)}`}
                    >
                        {alttext ? alttext : text} {iconButton}
                    </Link>
                </div>
            );
        }
    } else {
        textComponent = (
            <div className={styles.linktext}>{alttext ? alttext : text}</div>
        );
    }

    return (
        <div className={styles.longtextwrapper}>
            {textComponent}
            {icon}
        </div>
    );
}

export default Longtext;
