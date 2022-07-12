// Copyright (c) 2022, Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import { useState, useCallback, useEffect } from 'react';

import {
    FALLBACK_IMAGE,
    ImageModClient,
} from '../../utils/imageModeratorClient';
import { transformURL } from '../../utils/stringUtils';

import styles from './DisplayBox.module.css';

function DisplayBox({ display }: { display: string }) {
    const [hasDisplayLoaded, setHasDisplayLoaded] = useState(false);
    const [hasFailedToLoad, setHasFailedToLoad] = useState(false);

    const [hasImgBeenChecked, setHasImgBeenChecked] = useState(false);
    const [imgAllowState, setImgAllowState] = useState(false);

    const imageStyle = hasDisplayLoaded ? {} : { display: 'none' };
    const handleImageLoad = useCallback(
        () => {
            setHasDisplayLoaded(true);
            setHasFailedToLoad(false);
        },
        [setHasDisplayLoaded]
    );

    useEffect(() => {
        setHasFailedToLoad(false);
        setHasImgBeenChecked(false);
        setImgAllowState(false);

        new ImageModClient()
            .checkImage(transformURL(display))
            .then(({ ok }) => {
                setImgAllowState(ok);
            })
            .catch((error) => {
                console.warn(error);
                // default to allow, so a broken img check service doesn't break NFT display
                setImgAllowState(true);
            })
            .finally(() => {
                setHasImgBeenChecked(true);
            });
    }, [display]);

    const handleImageFail = useCallback(
        (error) => {
            console.log(error);
            setHasDisplayLoaded(true);
            setHasFailedToLoad(true);
        },
        [setHasFailedToLoad]
    );

    const loadedWithoutAllowedState = hasDisplayLoaded && !imgAllowState;

    let showAutoModNotice =
        !hasFailedToLoad && hasImgBeenChecked && !imgAllowState;

    if (loadedWithoutAllowedState && hasImgBeenChecked) {
        display = FALLBACK_IMAGE;
        showAutoModNotice = true;
    }

    return (
        <div className={styles['display-container']}>
            {!hasDisplayLoaded && !showAutoModNotice && (
                <div className={styles.imagebox} id="pleaseWaitImage">
                    image loading...
                </div>
            )}
            {hasFailedToLoad && !showAutoModNotice && (
                <div className={styles.imagebox} id="noImage">
                    No Image was Found
                </div>
            )}
            {!hasFailedToLoad && !showAutoModNotice && (
                <img
                    id="loadedImage"
                    className={styles.imagebox}
                    style={imageStyle}
                    alt="NFT"
                    src={transformURL(display)}
                    onLoad={handleImageLoad}
                    onError={handleImageFail}
                />
            )}
            {showAutoModNotice && (
                <div className={styles.automod} id="modnotice">
                    NFT image hidden
                </div>
            )}
        </div>
    );
}

export default DisplayBox;
