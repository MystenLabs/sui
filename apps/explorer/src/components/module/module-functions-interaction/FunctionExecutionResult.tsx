// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import {
    getExecutionStatusError,
    getObjectId,
    getTransactionDigest,
} from '@mysten/sui.js';

import { useTxEffectsObjectRefs } from './useTxEffectsObjectRefs';

import type { SuiTransactionResponse, SuiObjectRef } from '@mysten/sui.js';

import { Banner } from '~/ui/Banner';
import { LinkGroup } from '~/ui/LinkGroup';

function toObjectLink(object: SuiObjectRef) {
    return {
        text: getObjectId(object),
        to: `/object/${encodeURIComponent(getObjectId(object))}`,
    };
}

type FunctionExecutionResultProps = {
    result: SuiTransactionResponse | null;
    error: string | false;
    onClear: () => void;
};

export function FunctionExecutionResult({
    error,
    result,
    onClear,
}: FunctionExecutionResultProps) {
    const adjError =
        error || (result && getExecutionStatusError(result)) || null;
    const variant = adjError ? 'error' : 'message';
    const createdObjects = useTxEffectsObjectRefs(result, 'created');
    const mutatedObjects = useTxEffectsObjectRefs(result, 'mutated');
    return (
        <Banner
            icon={null}
            fullWidth
            variant={variant}
            spacing="lg"
            onDismiss={onClear}
        >
            <div className="space-y-4 text-bodySmall">
                <LinkGroup
                    title="Transaction ID"
                    links={
                        result
                            ? [
                                  {
                                      text: getTransactionDigest(result),
                                      to: `/transaction/${encodeURIComponent(
                                          getTransactionDigest(result)
                                      )}`,
                                  },
                              ]
                            : []
                    }
                />
                <LinkGroup
                    title="Created"
                    links={createdObjects.map(toObjectLink)}
                />
                <LinkGroup
                    title="Updated"
                    links={mutatedObjects.map(toObjectLink)}
                />
                <LinkGroup title="Transaction failed" text={adjError} />
            </div>
        </Banner>
    );
}
