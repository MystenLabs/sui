// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import { transformURL } from '../../../utils/stringUtils';
import { type Data, type DataType } from '../OwnedObjectConstants';

import { useImageMod } from '~/hooks/useImageMod';
import { ObjectDetails } from '~/ui/ObjectDetails';

function OwnedNFT(entryObj: Data) {
    const url = transformURL(entryObj.display ?? '');
    const { data: allowed } = useImageMod({ url });

    return (
        <ObjectDetails
            id={entryObj.id}
            name={entryObj.name}
            type={entryObj.name ?? entryObj.Type}
            image={url}
            variant="small"
            nsfw={!allowed}
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
