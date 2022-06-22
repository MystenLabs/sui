// Copyright (c) 2022, Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import { useState, useCallback, useEffect } from 'react';

import { ImageModClient } from '../../utils/imageModeratorClient';
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
                //setHasImgBeenChecked(true);
                setImgAllowState(ok);
            })
            .catch((error) => {
                //setHasImgBeenChecked(true);
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
    const shouldStillBlur = loadedWithoutAllowedState && hasImgBeenChecked;
    // if we've loaded the display image but the check hasn't returned, display a blurry version
    let imgClass = shouldBlur ? styles.imageboxblur : styles.imagebox;
    // if we've loaded the display image and the check did not pass, blur harder & stop animation
    imgClass = shouldStillBlur ? styles.imageblurstill : imgClass;

    return (
        <div className={styles['display-container']}>
            {!hasDisplayLoaded && (
                <div className={styles.imagebox} id="pleaseWaitImage">
                    image loading...
                </div>
            )}
            {hasFailedToLoad && (
                <div className={styles.imagebox} id="noImage">
                    No Image was Found
                </div>
            )}
            {!hasFailedToLoad && hasImgBeenChecked && !imgAllowState && (
                <div className={styles.automod} id="modnotice">
                    image hidden by automod
                </div>
            )}
            {!hasFailedToLoad && (
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
        </div>
    );
}

export default DisplayBox;
