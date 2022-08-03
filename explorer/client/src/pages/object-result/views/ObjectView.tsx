// Copyright (c) 2022, Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import { type DataType } from '../ObjectResultType';
import PkgView from './PkgView';
import TokenView from './TokenView';
import ObjHeader from './shared/ObjHeader';

function ObjectView({ data }: { data: DataType }) {
    const nameKeyValue = Object.entries(data.data?.contents)
        .filter(([key, _]) => key === 'name')
        .map(([_, value]) => value)[0];

    if (data.objType === 'Move Package') {
        return (
            <>
                <ObjHeader
                    data={{
                        objId: data.id,
                        objKind: 'Package',
                        objName: nameKeyValue,
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
                        objName: nameKeyValue,
                    }}
                />
                <TokenView data={data} name={nameKeyValue} />
            </>
        );
    }
}

export default ObjectView;
