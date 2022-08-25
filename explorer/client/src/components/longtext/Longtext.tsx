// Copyright (c) 2022, Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import { useCallback, useState, useContext } from 'react';
import { Link, useNavigate } from 'react-router-dom';

import { ReactComponent as ContentCopyIcon } from '../../assets/SVGIcons/Copy.svg';
import { ReactComponent as ContentForwardArrowDark } from '../../assets/SVGIcons/forward-arrow-dark.svg';
import { NetworkContext } from '../../context';
import { navigateWithCategory } from '../../utils/searchUtil';
import ExternalLink from '../external-link/ExternalLink';

import styles from './Longtext.module.css';

function Longtext({
    text,
    category = 'owner',
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
        | 'validators'
        | 'owner';
    isLink?: boolean;
    alttext?: string;
    isCopyButton?: boolean;
    showIconButton?: boolean;
}) {
    const [isCopyIcon, setCopyIcon] = useState(true);
    const navigate = useNavigate();
    const [network] = useContext(NetworkContext);

    const handleCopyEvent = useCallback(() => {
        navigator.clipboard.writeText(text);
        setCopyIcon(false);
        setTimeout(() => setCopyIcon(true), 1000);
    }, [setCopyIcon, text]);

    const navigateToOwner = useCallback(
        (input: string) => () =>
            navigateWithCategory(input, 'owner', network).then((resp: any) =>
                navigate(
                    `../${resp.category}/${encodeURIComponent(resp.input)}`,
                    {
                        state: resp.result,
                    }
                )
            ),
        [network, navigate]
    );

    let icon;
    let iconButton = <></>;

    if (isCopyButton) {
        if (isCopyIcon) {
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
        if (category === 'owner') {
            textComponent = (
                <span
                    className={styles.longtext}
                    onClick={navigateToOwner(text)}
                >
                    {alttext ? alttext : text} {iconButton}
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
        textComponent = (
            <span className={styles.linktext}>{alttext ? alttext : text}</span>
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
