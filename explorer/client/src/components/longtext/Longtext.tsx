// Copyright (c) 2022, Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import { useCallback, useState, useContext } from 'react';
import { Link, useNavigate } from 'react-router-dom';

import { ReactComponent as ContentForwardArrowDark } from '../../assets/SVGIcons/forward-arrow-dark.svg';
import { ReactComponent as ContentCopyIcon } from '../../assets/content_copy_black_18dp.svg';
import { NetworkContext } from '../../context';
import { navigateWithUnknown } from '../../utils/searchUtil';
import ExternalLink from '../external-link/ExternalLink';

import styles from './Longtext.module.css';

function Longtext({
    text,
    category = 'unknown',
    isLink = true,
    alttext = '',
    isCopyButton = true,
    showIconButton = false,
}: {
    text: string;
    category:
        | 'objects'
        | 'transactions'
        | 'addresses'
        | 'ethAddress'
        | 'unknown';
    isLink?: boolean;
    alttext?: string;
    isCopyButton?: boolean;
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

    if (isCopyButton) {
        if (pleaseWait) {
            icon = <span className={styles.copied}>&#8987; Please Wait</span>;
        } else if (isCopyIcon) {
            icon = (
                <span className={styles.copy} onClick={handleCopyEvent}>
                    <ContentCopyIcon />
                </span>
            );
        } else {
            icon = <span className={styles.copied}>&#10003; Copied</span>;
        }
    } else {
        icon = <></>;
    }

    if (showIconButton) {
        iconButton = <ContentForwardArrowDark />;
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
                <span className={styles.longtext} onClick={navigateUnknown}>
                    {alttext ? alttext : text}
                </span>
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
                <Link
                    className={styles.longtext}
                    to={`/${category}/${encodeURIComponent(text)}`}
                >
                    {alttext ? alttext : text} {iconButton}
                </Link>
            );
        }
    } else {
        textComponent = <span>{alttext ? alttext : text}</span>;
    }

    return (
        <>
            {textComponent}&nbsp;{icon}
        </>
    );
}

export default Longtext;
