// Copyright (c) 2022, Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

<<<<<<< HEAD:explorer/client/src/components/displaybox/DisplayBox.tsx
import { useState, useCallback, useEffect } from 'react';

import { processDisplayValue } from '../../utils/stringUtils';
=======
import { useState, useCallback } from 'react';
import { GetObjectInfoResponse } from 'sui.js';

import { asciiFromNumberBytes } from '../../utils/stringUtils';
>>>>>>> explorer-jsonrpc:explorer/client/src/pages/object-result/DisplayBox.tsx

import styles from './DisplayBox.module.css';

//TO DO - display smart contract info; see mock_data.json for example smart contract data
//import 'ace-builds/src-noconflict/theme-github';
//import AceEditor from 'react-ace';

<<<<<<< HEAD:explorer/client/src/components/displaybox/DisplayBox.tsx
function SmartContractBox({
    display,
}: {
    display: string | { bytes: number[] };
}) {
=======
function SmartContractBox({ data }: { data: GetObjectInfoResponse }) {
>>>>>>> explorer-jsonrpc:explorer/client/src/pages/object-result/DisplayBox.tsx
    return (
        <div className={styles.imagebox}>
            Displaying Smart Contracts Not yet Supported
        </div>
    );
    /*
           return (
                         <div className={styles['display-container']}>
                             <AceEditor
                                 theme="github"
                                 value={data.data.contents.display?.data}
                                 showGutter={true}
                                 readOnly={true}
                                 fontSize="0.8rem"
                                 className={styles.codebox}
                             />
                         </div>
                     );
                     */
}

<<<<<<< HEAD:explorer/client/src/components/displaybox/DisplayBox.tsx
function DisplayBox({
    display,
    tag,
}: {
    display: string | { bytes: number[] };
    tag: 'imageURL' | 'moveScript';
}) {
    const [hasDisplayLoaded, setHasDisplayLoaded] = useState(false);
    const [hasFailedToLoad, setHasFailedToLoad] = useState(false);

=======
function DisplayBox({ data }: { data: GetObjectInfoResponse }) {
    const [hasDisplayLoaded, setHasDisplayLoaded] = useState(false);
    const [hasFailedToLoad, setHasFailedToLoad] = useState(false);

    // @ts-ignore
    const contents = data.details.object;

>>>>>>> explorer-jsonrpc:explorer/client/src/pages/object-result/DisplayBox.tsx
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

    if (tag === 'moveScript') {
        return <SmartContractBox display={display} />;
    }

    if (tag === 'imageURL') {
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
                        src={processDisplayValue(display)}
                        onLoad={handleImageLoad}
                        onError={handleImageFail}
                    />
                )}
            </div>
        );
    }
    return <div />;
}

export default DisplayBox;
