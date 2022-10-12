// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import { extractName } from '../../../utils/objectUtils';
import { type DataType } from '../ObjectResultType';
import PkgView from './PkgView';
import TokenView from './TokenView';
import ObjHeader from './shared/ObjHeader';

function ObjectView({ data }: { data: DataType }) {
    const name = extractName(data.data?.contents);

    if (data.objType === 'Move Package') {
        return (
            <>
                <ObjHeader
                    data={{
                        objId: data.id,
                        objKind: 'Package',
                        objName: name,
                    }}
                />
                <PkgView data={data} />
            </>
        );
    } else {
        return (
            <>
                <ObjHeader
                    data={{
                        objId: data.id,
                        objKind: 'Object',
                        objName: name,
                    }}
                />
                <TokenView data={data} />
            </>
        );
    }
}

export default ObjectView;
