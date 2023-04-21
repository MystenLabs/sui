// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import type { SuiObjectRef } from '@mysten/sui.js';

import styles from './TxLinks.module.css';

import { ExpandableList } from '~/ui/ExpandableList';
import { ObjectLink } from '~/ui/InternalLink';
import { IconTooltip } from '~/ui/Tooltip';

type Addresslist = {
    label: string;
    category?: string;
    links: SuiObjectRef[];
};
function TxLinks({ data }: { data: Addresslist }) {
    return (
        <div className={styles.mutatedcreatedlist}>
            <h3 className={styles.label}>{data.label}</h3>
            <div className={styles.objectidlists}>
                <ul>
                    <ExpandableList
                        defaultItemsToShow={3}
                        items={data.links.map((obj, idx) => (
                            <li key={idx}>
                                <div className="inline-flex items-center gap-1.5">
                                    <ObjectLink
                                        objectId={obj.objectId}
                                        noTruncate
                                    />
                                    <div className="h-4 w-4 leading-none text-gray-60 hover:text-steel">
                                        <IconTooltip
                                            tip={`VERSION ${obj.version}`}
                                        />
                                    </div>
                                </div>
                            </li>
                        ))}
                    />
                </ul>
            </div>
        </div>
    );
}

export default TxLinks;
