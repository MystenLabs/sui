// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import { useRpcClient } from '@mysten/core';
import {
    Coin,
    getObjectId,
    getObjectType,
    getObjectOwner,
} from '@mysten/sui.js';
import { useQuery } from '@tanstack/react-query';

import {
    parseImageURL,
    parseObjectType,
    extractName,
} from '../../utils/objectUtils';
import { transformURL } from '../../utils/stringUtils';
import OwnedObjectView from './views/OwnedObjectView';

import { useMultiGetObjects } from '~/hooks/useMultiGetObject';
import { Banner } from '~/ui/Banner';
import { LoadingSpinner } from '~/ui/LoadingSpinner';

function OwnedObject({ id, byAddress }: { id: string; byAddress: boolean }) {
    const rpc = useRpcClient();
    const { data, isLoading, isError } = useQuery(
        ['owned-object', id],
        async () =>
            byAddress
                ? rpc.getOwnedObjects({ owner: id })
                : rpc.getDynamicFields({ parentId: id }),
        { enabled: !!id }
    );

    let ids: string[];
    ids = data?.data.map(getObjectId) || [];

    const {
        data: multiGetObjects,
        isLoading: loadingGetMultiObjects,
        isError: errorGetMultiObject,
    } = useMultiGetObjects(ids);

    // TODO: change this view model
    const results = multiGetObjects
        ?.filter((resp) => {
            if (byAddress && getObjectType(resp) === 'moveObject') {
                const owner = getObjectOwner(resp);
                const addressOwner =
                    owner && owner !== 'Immutable' && 'AddressOwner' in owner
                        ? owner.AddressOwner
                        : null;
                return resp !== undefined && addressOwner === id;
            }
            return resp !== undefined;
        })
        .map((resp) => {
            const displayMeta =
                typeof resp.data === 'object' && 'display' in resp.data
                    ? resp.data.display
                    : undefined;
            const url = parseImageURL(displayMeta);
            return {
                id: getObjectId(resp),
                Type: parseObjectType(resp),
                _isCoin: Coin.isCoin(resp),
                display: url ? transformURL(url) : undefined,
                balance: Coin.getBalance(resp),
                name: extractName(displayMeta) || '',
            };
        });

    if (isLoading || loadingGetMultiObjects) {
        return <LoadingSpinner text="Loading data" />;
    }

    if (isError || errorGetMultiObject) {
        <Banner variant="error" spacing="lg" fullWidth>
            Failed to find Owned Objects
        </Banner>;
    }

    return results ? <OwnedObjectView results={results} /> : null;
}

export default OwnedObject;
