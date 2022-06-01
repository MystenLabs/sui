// Copyright (c) 2022, Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import { useState, useCallback, useEffect } from 'react';

import { transformURL } from '../../utils/stringUtils';

import styles from './DisplayBox.module.css';

function DisplayBox({ display }: { display: string }) {
    const [hasDisplayLoaded, setHasDisplayLoaded] = useState(false);
    const [hasFailedToLoad, setHasFailedToLoad] = useState(false);

    const imageStyle = hasDisplayLoaded ? {} : { display: 'none' };
    const handleImageLoad = useCallback(
        () => setHasDisplayLoaded(true),
        [setHasDisplayLoaded]
    );

    useEffect(() => {
        setHasFailedToLoad(false);
    }, [display]);

    const handleImageFail = useCallback(
        (error) => {
            console.log(error);
            setHasDisplayLoaded(true);
            setHasFailedToLoad(true);
        },
        [setHasFailedToLoad]
    );

    return (
        <div className={styles['display-container']}>
            {!hasDisplayLoaded && (
                <div className={styles.imagebox} id="pleaseWaitImage">
                    Please wait for display to load
                </div>
            )}
            {hasFailedToLoad ? (
                <div className={styles.imagebox} id="noImage">
                    No Image was Found
                </div>
            ) : (
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
        </div>
    );
}

export default DisplayBox;
