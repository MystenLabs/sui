// Copyright (c) 2022, Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import { useState, useCallback } from 'react';
import { GetObjectInfoResponse } from 'sui.js';

import { asciiFromNumberBytes } from '../../utils/stringUtils';

import styles from './ObjectResult.module.css';

//TO DO - display smart contract info; see mock_data.json for example smart contract data
//import 'ace-builds/src-noconflict/theme-github';
//import AceEditor from 'react-ace';

function SmartContractBox({ data }: { data: GetObjectInfoResponse }) {
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

function DisplayBox({ data }: { data: GetObjectInfoResponse }) {
    const [hasDisplayLoaded, setHasDisplayLoaded] = useState(false);
    const [hasFailedToLoad, setHasFailedToLoad] = useState(false);

    // @ts-ignore
    const contents = data.details.object;

    const imageStyle = hasDisplayLoaded ? {} : { display: 'none' };
    const handleImageLoad = useCallback(
        () => setHasDisplayLoaded(true),
        [setHasDisplayLoaded]
    );

    const handleImageFail = useCallback(
        (error) => {
            console.log(error);
            setHasDisplayLoaded(true);
            setHasFailedToLoad(true);
        },
        [setHasFailedToLoad]
    );

    const IS_SMART_CONTRACT = (data: any) =>
        data?.data?.contents?.display?.category === 'moveScript';

    if (IS_SMART_CONTRACT(data)) {
        return <SmartContractBox data={data} />;
    }

    if (contents.display) {
        if (typeof contents.display === 'object' && 'bytes' in contents.display)
            contents.display = asciiFromNumberBytes(contents.display.bytes);

        return (
            <div className={styles['display-container']}>
                {!hasDisplayLoaded && (
                    <div className={styles.imagebox}>
                        Please wait for display to load
                    </div>
                )}
                {hasFailedToLoad && (
                    <div className={styles.imagebox}>No Image was Found</div>
                )}
                {!hasFailedToLoad && (
                    <img
                        className={styles.imagebox}
                        style={imageStyle}
                        alt="NFT"
                        src={contents.display}
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
