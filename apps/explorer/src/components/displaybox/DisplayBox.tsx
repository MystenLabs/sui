// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import { useState, useCallback, useEffect } from 'react';

import { ReactComponent as BrokenImage } from '../../assets/SVGIcons/24px/NFTTypeImage.svg';
import {
    FALLBACK_IMAGE,
    ImageModClient,
} from '../../utils/imageModeratorClient';
import { transformURL, genFileTypeMsg } from '../../utils/stringUtils';

import styles from './DisplayBox.module.css';

import { LoadingSpinner } from '~/ui/LoadingSpinner';

function ShowBrokenImage({ onClick }: { onClick?: () => void }) {
    return (
        <div
            className={`${styles.imagebox} ${styles.brokenimage}`}
            id="noImage"
            onClick={onClick}
        >
            <div>
                <BrokenImage />
            </div>
        </div>
    );
}

function DisplayBox({
    display,
    caption,
    fileInfo,
    modalImage,
}: {
    display: string | undefined;
    caption?: string;
    fileInfo?: string;
    modalImage?: [boolean, (hasClickedImage: boolean) => void];
}) {
    if (!display) return <ShowBrokenImage />;

    return (
        <DisplayBoxWString
            display={display}
            caption={caption}
            fileInfo={fileInfo}
            modalImage={modalImage}
        />
    );
}

function DisplayBoxWString({
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
            genFileTypeMsg(display, controller.signal)
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

    // When the image is clicked, indicating that it should be full screen, this is communicated outside the component:
    useEffect(() => {
        if (setIsFullScreen) {
            setIsFullScreen(hasClickedImage);
        }
    }, [hasClickedImage, setIsFullScreen]);

    // When a button is clicked outside the component that signals that the image should be fullscreen,
    // this useEffect uses that signal to force the image to go full screen:
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

    const [isFadeEffect, setFadeEffect] = useState(false);

    const handleImageClick = useCallback(() => {
        setHasClickedImage((prevHasClicked) => !prevHasClicked);
        setFadeEffect(true);
        // If changing the animation duration here do
        // not forget to change the duration in the CSS class .fade
        setTimeout(() => setFadeEffect(false), 300);
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
        (error: unknown) => {
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
                        className={`${styles.modalcontainer}
                          ${isFadeEffect ? styles.fade : ''}`}
                        onClick={handleImageClick}
                    >
                        <div className={styles.modal}>
                            <figure className={styles.fig}>
                                <div className={styles.imageandcross}>
                                    {hasFailedToLoad ? (
                                        <ShowBrokenImage />
                                    ) : (
                                        <img
                                            data-testid="loadedImage"
                                            className={styles.largeimage}
                                            alt="NFT"
                                            src={transformURL(display)}
                                        />
                                    )}
                                    <span className={styles.desktopcross}>
                                        <span className={styles.cross}>
                                            &times;
                                        </span>
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
                                <div className={styles.mobilecross}>
                                    <span className={styles.cross}>
                                        &times;
                                    </span>
                                </div>
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
                            <LoadingSpinner />
                        </div>
                    )}
                    {hasFailedToLoad && (
                        <ShowBrokenImage onClick={handleImageClick} />
                    )}
                    {!hasFailedToLoad && (
                        <img
                            data-testid="loadedImage"
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
