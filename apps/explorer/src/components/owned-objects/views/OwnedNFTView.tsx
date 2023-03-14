// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import { type Data, type DataType } from '../OwnedObjectConstants';

import { useImage } from '~/hooks/useImage';
import { ObjectDetails } from '~/ui/ObjectDetails';

function OwnedNFT(entryObj: Data) {
    const { url, nsfw } = useImage({
        src: entryObj.display ?? '',
        moderate: true,
    });

    return (
        <ObjectDetails
            id={entryObj.id}
            name={entryObj.name}
            type={entryObj.name ?? entryObj.Type}
            image={url}
            variant="small"
            nsfw={nsfw}
        />
    );
}

export default function OwnedNFTView({ results }: { results: DataType }) {
    const res = [
        {
            id: '1',
            _isCoin: false,
            Type: 'NFT',
            display:
                'https://images.unsplash.com/photo-1678716708878-880e14271fdd?ixlib=rb-4.0.3&ixid=MnwxMjA3fDB8MHxwaG90by1wYWdlfHx8fGVufDB8fHx8&auto=format&fit=crop&w=930&q=80',
            name: 'test',
        },
    ];
    return (
        <div className="mb-10 grid grid-cols-2 gap-4">
            {res.map((entryObj) => (
                <OwnedNFT key={entryObj.id} {...entryObj} />
            ))}
        </div>
    );
}
