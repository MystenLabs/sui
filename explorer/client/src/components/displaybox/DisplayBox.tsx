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
        () => setHasDisplayLoaded(true),
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
    const shouldBlur = loadedWithoutAllowedState && !hasImgBeenChecked;
    const shouldBlock = loadedWithoutAllowedState && hasImgBeenChecked;
    // if we've loaded the display image but the check hasn't returned, display a blurry version
    let imgClass = shouldBlur ? styles.imageboxblur : styles.imagebox;
    // if we've loaded the display image and the check did not pass,
    // stop blur animation and use a fallback image
    imgClass = shouldBlock ? styles.imagebox : imgClass;

    let showAutoModNotice =
        !hasFailedToLoad && hasImgBeenChecked && !imgAllowState;

    if (loadedWithoutAllowedState && hasImgBeenChecked) {
        display = FALLBACK_IMAGE;
        imgClass = styles.imagebox;
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
            {hasDisplayLoaded && !hasFailedToLoad && !showAutoModNotice && (
                <img
                    id="loadedImage"
                    className={imgClass}
                    style={imageStyle}
                    alt="NFT"
                    src={transformURL(display)}
                    onLoad={handleImageLoad}
                    onError={handleImageFail}
                />
            )}
            {showAutoModNotice && (
                <div className={styles.automod} id="modnotice">
                    image hidden by automod
                </div>
            )}
        </div>
    );
}

export default DisplayBox;
