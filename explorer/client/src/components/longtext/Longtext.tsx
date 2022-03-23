import { useCallback, useState } from 'react';
import { Link, useNavigate } from 'react-router-dom';

import { ReactComponent as ContentCopyIcon } from '../../assets/content_copy_black_18dp.svg';
import { navigateWithUnknown } from '../../utils/utility_functions';
import ExternalLink from '../external-link/ExternalLink';

import styles from './Longtext.module.css';

function Longtext({
    text,
    category = 'unknown',
    isLink = true,
}: {
    text: string;
    category:
        | 'objects'
        | 'transactions'
        | 'addresses'
        | 'ethAddress'
        | 'unknown'
        | 'objectId';
    isLink?: boolean;
}) {
    const [isCopyIcon, setCopyIcon] = useState(true);
    const navigate = useNavigate();

    const handleCopyEvent = useCallback(() => {
        navigator.clipboard.writeText(text);
        setCopyIcon(false);
        setTimeout(() => setCopyIcon(true), 1000);
    }, [setCopyIcon, text]);

    let icon;

    if (isCopyIcon) {
        icon = (
            <span className={styles.copy} onClick={handleCopyEvent}>
                <ContentCopyIcon />
            </span>
        );
    } else {
        icon = <span className={styles.copied}>&#10003; Copied</span>;
    }


    const navigateUnknown = useCallback(() => navigateWithUnknown(text, navigate), [text, navigate]);
    let textComponent;
    if (isLink) {
        if (category === 'objectId') {
            textComponent = (
                <span
                    className={styles.longtext}
                >
                    <a href={'/objects/' + text}>
                        {text}
                    </a>
                </span>
            );
        } else if (category === 'unknown') {
            textComponent = (
                <span
                    className={styles.longtext}
                    onClick={navigateUnknown}
                >
                    {text}
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
                <Link className={styles.longtext} to={`/${category}/${text}`}>
                    {text}
                </Link>
            );
        }
    } else {
        textComponent = <span>{text}</span>;
    }

    return (
        <>
            {textComponent}&nbsp;{icon}
        </>
    );
}

export default Longtext;
