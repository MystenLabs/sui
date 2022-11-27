// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import { getExecutionStatusError, getTransactionDigest } from '@mysten/sui.js';

import { useTxEffectsObjectRefs } from './useTxEffectsObjectRefs';

import type { SuiTransactionResponse } from '@mysten/sui.js';

import Longtext from '~/components/longtext/Longtext';
import { Banner } from '~/ui/Banner';

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
    const createdObjs = useTxEffectsObjectRefs(result, 'created');
    const mutatedObjs = useTxEffectsObjectRefs(result, 'mutated');
    const labelWithLinks = result
        ? [
              {
                  title: 'Transaction ID',
                  links: [
                      {
                          text: getTransactionDigest(result),
                          category: 'transactions',
                      },
                  ],
              },
              createdObjs.length && { title: 'Created', links: createdObjs },
              mutatedObjs.length && { title: 'Updated', links: mutatedObjs },
          ].filter(Boolean)
        : adjError
        ? [{ title: 'Transaction failed', text: adjError }]
        : [];
    return (
        <Banner
            icon={null}
            fullWidth
            variant={variant}
            spacing="lg"
            onDismiss={onClear}
        >
            <div className="space-y-4 text-bodySmall">
                {labelWithLinks.map((groupLinks) => {
                    if (!groupLinks) {
                        return null;
                    }
                    return (
                        <div className="space-y-3" key={groupLinks.title}>
                            {groupLinks.title ? (
                                <div className="font-semibold text-gray-90">
                                    {groupLinks.title}
                                </div>
                            ) : null}
                            {'links' in groupLinks
                                ? groupLinks.links.map((aLink) => {
                                      const text =
                                          'text' in aLink
                                              ? aLink.text
                                              : aLink.objectId;
                                      return (
                                          <div
                                              className="font-mono font-medium"
                                              key={text}
                                          >
                                              <Longtext
                                                  text={text}
                                                  category={
                                                      'category' in aLink
                                                          ? (aLink.category as any)
                                                          : 'objects'
                                                  }
                                                  isLink
                                              />
                                          </div>
                                      );
                                  })
                                : null}
                            {'text' in groupLinks ? (
                                <div className="text-p2 font-medium text-gray-90">
                                    {groupLinks.text}
                                </div>
                            ) : null}
                        </div>
                    );
                })}
            </div>
        </Banner>
    );
}
