import { ReactComponent as ObjIcon } from '../../../../assets/SVGIcons/Call.svg';
import GoBack from '../../../../components/goback/GoBack';
import Longtext from '../../../../components/longtext/Longtext';

import styles from './ObjHeader.module.css';

type ObjHeaderData = {
    objId: string;
    objKind: 'Object' | 'Package';
    objName?: string;
};

function ObjAddressHeader({ data }: { data: ObjHeaderData }) {
    const ObjectKindName = data.objKind;
    return (
        <div className={styles.objheader}>
            <GoBack />
            <div className={styles.objtypes}>
                <ObjIcon /> {ObjectKindName}
            </div>
            <div className={styles.objaddress}>
                <Longtext text={data.objId} category="objects" isLink={false} />
            </div>
            {data.objName && (
                <div className={styles.objname}>{data.objName}</div>
            )}
        </div>
    );
}

export default ObjAddressHeader;
