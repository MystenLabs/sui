// Copyright (c) 2022, Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import { ReactComponent as ObjIcon } from '../../../../assets/SVGIcons/Call.svg';
import Longtext from '../../../../components/longtext/Longtext';
import resultheaderstyle from '../../../../styles/resultheader.module.css';

import styles from './ObjHeader.module.css';

type ObjHeaderData = {
    objId: string;
    objKind: 'Object' | 'Package';
    objName?: string;
};

function ObjAddressHeader({ data }: { data: ObjHeaderData }) {
    return (
        <div
            className={`${resultheaderstyle.btmborder} ${styles.objcontainer}`}
        >
            <div className={resultheaderstyle.category}>
                <ObjIcon /> {data.objKind}
            </div>
            <div className={resultheaderstyle.address}>
                <Longtext text={data.objId} category="objects" isLink={false} />
            </div>
            {data.objName && (
                <div className={styles.objname}>{data.objName}</div>
            )}
        </div>
    );
}

export default ObjAddressHeader;
