// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import { Dialog, Transition } from '@headlessui/react';
import { useState, useCallback, useEffect, Fragment } from 'react';

import { ReactComponent as BrokenImage } from '../assets/SVGIcons/24px/NFTTypeImage.svg';
import { FALLBACK_IMAGE, ImageModClient } from '../utils/imageModeratorClient';
import { transformURL, genFileTypeMsg } from '../utils/stringUtils';

function ShowBrokenImage({ onClick }: { onClick?: () => void }) {
    return (
        <div
            className="h-full w-full flex items-center justify-center stroke-sui-grey-65 bg-sui-grey-45 rounded-md"
            id="noImage"
            onClick={onClick}
        >
            <div>
                <BrokenImage />
            </div>
        </div>
    );
}

export type DisplayBoxProps = {
    display: string | undefined;
    caption?: string;
    fileInfo?: string;
    modalImage?: [boolean, (hasClickedImage: boolean) => void];
};

export function DisplayBox({
    display,
    caption,
    fileInfo,
    modalImage,
}: DisplayBoxProps) {
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
            <div
                className="border-0 border-b-2 border-solid border-gray-300 w-1/2 ibg-modnotice relative text-center content-center align-middle m-auto p-4 mt-8"
                id="modnotice"
            >
                NFT image hidden
            </div>
        );
    } else {
        return (
            <>
                <Transition appear show={hasClickedImage} as={Fragment}>
                    <Dialog as="div" onClose={handleImageClick}>
                        <Transition.Child
                            enter="ease-linear duration-300"
                            enterFrom="opacity-0"
                            enterTo="opacity-100"
                            as={Fragment}
                        >
                            <div className="fixed inset-0 bg-sui-grey-100 z-20 bg-opacity-90" />
                        </Transition.Child>
                        <Transition.Child
                            className="w-full h-full z-50 fixed left-0 top-0 text-center justify-center flex"
                            enter="ease-linear duration-300"
                            enterFrom="opacity-0"
                            enterTo="opacity-100"
                        >
                            <Dialog.Panel
                                as="div"
                                className="mt-auto mb-15 sm:my-auto sm:relative sm:left-5"
                            >
                                <div className="flex">
                                    {hasFailedToLoad ? (
                                        <ShowBrokenImage />
                                    ) : (
                                        <img
                                            id="loadedImage"
                                            className="max-w-[80vw] max-h-[80vh] z-50 self-start border-0 rounded-sm"
                                            alt="Object's NFT"
                                            src={transformURL(display)}
                                        />
                                    )}
                                    <button
                                        onClick={handleImageClick}
                                        className="sr-only"
                                        type="button"
                                    >
                                        Close Dialog
                                    </button>
                                    <span
                                        className="hidden sm:block sm:ml-2 sm:mr-0"
                                        onClick={handleImageClick}
                                        aria-hidden
                                    >
                                        <span className="text-offwhite bg-sui-grey-90 h-7.5 w-7.5 flex justify-center items-center rounded-full text-2xl mx-auto z-50">
                                            &times;
                                        </span>
                                    </span>
                                </div>
                                <Dialog.Description as="div">
                                    {caption && (
                                        <div className="break-words max-w-[90vw] relative text-offwhite z-50 text-center sm:text-left text-2xl font-semibold mt-5">
                                            {caption}{' '}
                                        </div>
                                    )}
                                    <div className="break-words max-w-[90vw] relative text-offwhite z-50 text-center sm:text-left text-2xl font-semibold mt-5 text-sm text-sui-grey-60 font-medium mt-0">
                                        {fileType}
                                    </div>
                                </Dialog.Description>
                                <div
                                    className="block mx-auto mt-[10vh] sm:hidden"
                                    aria-hidden
                                >
                                    <span className="text-offwhite bg-sui-grey-90 h-7.5 w-7.5 flex justify-center items-center rounded-full text-2xl mx-auto z-50">
                                        &times;
                                    </span>
                                </div>
                            </Dialog.Panel>
                        </Transition.Child>
                    </Dialog>
                </Transition>

                <div id="displayContainer">
                    {!hasDisplayLoaded && (
                        <div
                            className="h-full w-full flex items-center justify-center"
                            id="pleaseWaitImage"
                        >
                            Image Loading...
                        </div>
                    )}
                    {hasFailedToLoad && (
                        <ShowBrokenImage onClick={handleImageClick} />
                    )}
                    {!hasFailedToLoad && (
                        <img
                            id="loadedImage"
                            className="object-fill cursor-pointer rounded-md"
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
