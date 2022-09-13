// Copyright (c) 2022, Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import { trimStdLibPrefix, alttextgen } from '../../../utils/stringUtils';
import DisplayBox from '../../displaybox/DisplayBox';
import Longtext from '../../longtext/Longtext';
import { type DataType } from '../OwnedObjectConstants';

import styles from '../styles/OwnedObjects.module.css';

export default function OwnedNFTView({ results }: { results: DataType }) {
    return (
        <div id="ownedObjects" className={styles.ownedobjects}>
            {results.map((entryObj, index1) => (
                <div className={styles.objectbox} key={`object-${index1}`}>
                    {entryObj.display !== undefined && (
                        <div className={styles.previewimage}>
                            <DisplayBox display={entryObj.display} />
                        </div>
                    )}
                    <div className={styles.textitem}>
                        <div>
                            <Longtext
                                text={entryObj.id}
                                category="objects"
                                alttext={alttextgen(entryObj.id)}
                            />
                        </div>
                        <div className={styles.typevalue}>
                            {trimStdLibPrefix(entryObj.Type)}
                        </div>
                    </div>
                </div>
            ))}
        </div>
    );
}
