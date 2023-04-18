// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import { trimStdLibPrefix } from '../../../utils/stringUtils';
import DisplayBox from '../../displaybox/DisplayBox';
import { type DataType } from '../OwnedObjectConstants';

import styles from '../styles/OwnedObjects.module.css';

import { ObjectLink } from '~/ui/InternalLink';
import { Text } from '~/ui/Text';

export default function OwnedNFTView({ results }: { results: DataType }) {
    return (
        <div id="ownedObjects" className={styles.ownedobjects}>
            {results.map((entryObj, index1) => (
                <div className={styles.objectbox} key={`object-${index1}`}>
                    <div className={styles.previewimage}>
                        <DisplayBox display={entryObj.display} />
                    </div>
                    <div className={styles.textitem}>
                        {entryObj.name && (
                            <div className={styles.name}>{entryObj.name}</div>
                        )}
                        <div>
                            <ObjectLink objectId={entryObj.id} />
                        </div>
                        <div className={styles.typevalue}>
                            <Text variant="pBodySmall/medium">
                                {trimStdLibPrefix(entryObj.Type)}
                            </Text>
                        </div>
                    </div>
                </div>
            ))}
        </div>
    );
}
