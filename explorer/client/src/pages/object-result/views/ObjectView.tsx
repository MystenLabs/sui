// Copyright (c) 2022, Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import { type DataType } from '../ObjectResultType';
import PkgView from './PkgView';
import TokenView from './TokenView';
import ObjHeader from './shared/ObjHeader';

function ObjectView({ data }: { data: DataType }) {
    if (data.objType === 'Move Package') {
        return (
            <>
                <ObjHeader
                    data={{
                        objId: data.id,
                        objKind: 'Package',
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
                    }}
                />
                <TokenView data={data} />
            </>
        );
    }

    /*
    const nameKeyValue = Object.entries(viewedData.data?.contents)
        .filter(([key, _]) => key === 'name')
        .map(([_, value]) => value);
*/
}

export default ObjectView;
