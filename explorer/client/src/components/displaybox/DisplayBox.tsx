// Copyright (c) 2022, Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import { useState, useCallback, useEffect } from 'react';

import { ReactComponent as BrokenImage } from '../../assets/SVGIcons/broken-image.svg';
import {
    FALLBACK_IMAGE,
    ImageModClient,
} from '../../utils/imageModeratorClient';
import { transformURL, extractFileType } from '../../utils/stringUtils';

import styles from './DisplayBox.module.css';

function DisplayBox({
    display,
    caption,
    fileInfo,
    modalImage,
}: {
    display: string;
    caption?: string;
    fileInfo?: string;
    modalImage?: [boolean, (hasClickedImage: boolean) => void];
}) {
    const [hasDisplayLoaded, setHasDisplayLoaded] = useState(false);
    const [hasFailedToLoad, setHasFailedToLoad] = useState(false);

    const [hasImgBeenChecked, setHasImgBeenChecked] = useState(false);
    const [imgAllowState, setImgAllowState] = useState(false);

    const [hasClickedImage, setHasClickedImage] = useState(false);

    const [fileType, setFileType] = useState('');

    useEffect(() => {
        if (!fileInfo) {
            const controller = new AbortController();
            extractFileType(display, controller.signal)
                .then((result) => setFileType(result))
                .catch((err) => console.log(err));

            return () => {
                controller.abort();
            };
        } else {
            setFileType(fileInfo);
        }
    }, [display, fileInfo]);

    const [isFullScreen, setIsFullScreen] = modalImage || [];

    // When image is clicked this is communicated outside the component:
    useEffect(() => {
        if (setIsFullScreen) {
            setIsFullScreen(hasClickedImage);
        }
    }, [hasClickedImage, setIsFullScreen]);

    // When a signal that the image should be full screen is received, this is
    // implemented:
    useEffect(() => {
        if (isFullScreen) {
            setHasClickedImage(isFullScreen);
        }
    }, [isFullScreen]);

    const imageStyle = hasDisplayLoaded ? {} : { display: 'none' };
    const handleImageLoad = useCallback(() => {
        setHasDisplayLoaded(true);
        setHasFailedToLoad(false);
    }, [setHasDisplayLoaded]);

    const handleImageClick = useCallback(() => {
        setHasClickedImage((prevHasClicked) => !prevHasClicked);
    }, []);

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

    if (showAutoModNotice) {
        return (
            <div className={styles['display-container']}>
                {showAutoModNotice && (
                    <div className={styles.automod} id="modnotice">
                        NFT image hidden
                    </div>
                )}
            </div>
        );
    } else {
        return (
            <>
                {hasClickedImage && (
                    <div
                        className={styles.modalcontainer}
                        onClick={handleImageClick}
                    >
                        <div className={styles.modal}>
                            <figure className={styles.fig}>
                                <div className={styles.imageandcross}>
                                    <img
                                        id="loadedImage"
                                        className={styles.largeimage}
                                        alt="NFT"
                                        src={transformURL(display)}
                                    />
                                    <span className={styles.cross}>
                                        &times;
                                    </span>
                                </div>
                                <figcaption>
                                    {caption && (
                                        <div className={styles.caption}>
                                            {caption}{' '}
                                        </div>
                                    )}
                                    <div className={styles.filetype}>
                                        {fileType}
                                    </div>
                                </figcaption>
                            </figure>
                        </div>
                        <div className={styles.detailsbg} />
                    </div>
                )}

                <div
                    className={styles['display-container']}
                    id="displayContainer"
                >
                    {!hasDisplayLoaded && (
                        <div className={styles.imagebox} id="pleaseWaitImage">
                            Image Loading...
                        </div>
                    )}
                    {hasFailedToLoad && (
                        <div className={styles.imagebox} id="noImage">
                            <BrokenImage />
                        </div>
                    )}
                    {!hasFailedToLoad && (
                        <img
                            id="loadedImage"
                            className={styles.smallimage}
                            style={imageStyle}
                            alt="NFT"
                            src={transformURL(display)}
                            onLoad={handleImageLoad}
                            onError={handleImageFail}
                            onClick={handleImageClick}
                        />
                    )}
                </div>
            </>
        );
    }
}

export default DisplayBox;
