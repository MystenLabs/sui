import { useState, useCallback } from 'react';

import { type AddressOwner } from '../../utils/internetapi/rpc';
import { asciiFromNumberBytes } from '../../utils/internetapi/utility_functions';

import styles from './ObjectResult.module.css';
type DataType = {
    id: string;
    category?: string;
    owner: string | AddressOwner;
    version: string;
    readonly?: string;
    objType: string;
    name?: string;
    ethAddress?: string;
    ethTokenId?: string;
    contract_id?: { bytes: string };
    data: {
        contents: {
            [key: string]: any;
        };
        owner?: { ObjectOwner: [] };
        tx_digest?: number[] | string;
    };
    loadState?: string;
};

function DisplayBox({ data }: { data: DataType }) {
    const [hasDisplayLoaded, setHasDisplayLoaded] = useState(false);

    const imageStyle = hasDisplayLoaded ? {} : { display: 'none' };

    const handleImageLoad = useCallback(
        () => setHasDisplayLoaded(true),
        [setHasDisplayLoaded]
    );

    if (data.data.contents.display) {
        if (
            typeof data.data.contents.display === 'object' &&
            'bytes' in data.data.contents.display
        )
            data.data.contents.display = asciiFromNumberBytes(
                data.data.contents.display.bytes
            );

        return (
            <div className={styles['display-container']}>
                {!hasDisplayLoaded && (
                    <div className={styles.imagebox}>
                        Please wait for display to load
                    </div>
                )}
                <img
                    className={styles.imagebox}
                    style={imageStyle}
                    alt="NFT"
                    src={data.data.contents.display}
                    onLoad={handleImageLoad}
                />
            </div>
        );
    }

    return <div />;
}

export default DisplayBox;
