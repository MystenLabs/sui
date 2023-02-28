// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import { type Data, type DataType } from '../OwnedObjectConstants';

import useMedia from '~/hooks/useMedia';
import { ObjectDetails } from '~/ui/ObjectDetails';

function OwnedNFT(entryObj: Data) {
    const { url, nsfw } = useMedia(entryObj.display ?? '');

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
    return (
        <div className="mb-10 grid grid-cols-2 gap-4">
            {results.map((entryObj) => (
                <OwnedNFT key={`object-${entryObj.id}`} {...entryObj} />
            ))}
        </div>
    );
}
