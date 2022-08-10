// Copyright (c) 2022, Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import { trimStdLibPrefix, truncate } from '../../../utils/stringUtils';
import DisplayBox from '../../displaybox/DisplayBox';
import Longtext from '../../longtext/Longtext';
import { type DataType } from '../OwnedObjectConstants';

import type BN from 'bn.js';

import styles from './OwnedObjects.module.css';

export default function OwnedNFTView({ results }: { results: DataType }) {
    const alttextgen = (value: number | string | boolean | BN): string =>
        truncate(String(value), 19);

    const lastRowHas2Elements = (itemList: any[]): boolean =>
        itemList.length % 3 === 2;

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
                        {Object.entries(entryObj).map(
                            ([key, value], index2) => (
                                <div key={`object-${index1}-${index2}`}>
                                    {(() => {
                                        switch (key) {
                                            case 'Type':
                                                if (entryObj._isCoin) {
                                                    break;
                                                } else {
                                                    return (
                                                        <span
                                                            className={
                                                                styles.typevalue
                                                            }
                                                        >
                                                            {trimStdLibPrefix(
                                                                value as string
                                                            )}
                                                        </span>
                                                    );
                                                }
                                            case 'balance':
                                                if (!entryObj._isCoin) {
                                                    break;
                                                } else {
                                                    return (
                                                        <div
                                                            className={
                                                                styles.coinfield
                                                            }
                                                        >
                                                            <div>Balance</div>
                                                            <div>
                                                                {String(value)}
                                                            </div>
                                                        </div>
                                                    );
                                                }
                                            case 'id':
                                                if (entryObj._isCoin) {
                                                    return (
                                                        <div
                                                            className={
                                                                styles.coinfield
                                                            }
                                                        >
                                                            <div>Object ID</div>
                                                            <div>
                                                                <Longtext
                                                                    text={String(
                                                                        value
                                                                    )}
                                                                    category="objects"
                                                                    isCopyButton={
                                                                        false
                                                                    }
                                                                    alttext={alttextgen(
                                                                        value
                                                                    )}
                                                                />
                                                            </div>
                                                        </div>
                                                    );
                                                } else {
                                                    return (
                                                        <Longtext
                                                            text={String(value)}
                                                            category="objects"
                                                            isCopyButton={false}
                                                            alttext={alttextgen(
                                                                value
                                                            )}
                                                        />
                                                    );
                                                }
                                            default:
                                                break;
                                        }
                                    })()}
                                </div>
                            )
                        )}
                    </div>
                </div>
            ))}
            {lastRowHas2Elements(results) && (
                <div className={`${styles.objectbox} ${styles.fillerbox}`} />
            )}
        </div>
    );
}
