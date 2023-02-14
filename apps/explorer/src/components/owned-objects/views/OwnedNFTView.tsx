// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import { transformURL } from '../../../utils/stringUtils';
import { type Data, type DataType } from '../OwnedObjectConstants';

import styles from '../styles/OwnedObjects.module.css';

import { useImageMod } from '~/hooks/useImageMod';
import { ObjectDetails } from '~/ui/ObjectDetails';

function OwnedNFT(entryObj: Data) {
    const url = transformURL(entryObj.display ?? '');
    const { data: allowed } = useImageMod({ url });

    return (
        <ObjectDetails
            name={entryObj.name}
            type={entryObj.name || ''}
            image={url}
            variant="small"
            nsfw={!allowed}
        />
    );
}

export default function OwnedNFTView({ results }: { results: DataType }) {
    return (
        <div id="ownedObjects" className={styles.ownedobjects}>
            {results.map((entryObj, index1) => (
                <OwnedNFT key={`object-${index1}`} {...entryObj} />
            ))}
        </div>
    );
}
