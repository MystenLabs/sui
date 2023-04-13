// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import { useRpcClient } from '@mysten/core';
import { useInfiniteQuery } from '@tanstack/react-query';
import { useRef, useCallback, useEffect } from 'react';

import { UnderlyingObjectCard } from './UnderlyingObjectCard';

import { DisclosureBox } from '~/ui/DisclosureBox';
import { ObjectLink } from '~/ui/InternalLink';
import { LoadingSpinner } from '~/ui/LoadingSpinner';
import { Tab, TabGroup, TabList, TabPanel, TabPanels } from '~/ui/Tabs';

const MAX_PAGE_SIZE = 20;

export function DynamicFieldsCard({ id }: { id: string }) {
    const rpc = useRpcClient();
    const {
        data,
        isInitialLoading,
        isFetchingNextPage,
        hasNextPage,
        fetchNextPage,
    } = useInfiniteQuery(
        ['dynamic-fields', id],
        ({ pageParam = null }) =>
            rpc.getDynamicFields({
                parentId: id,
                cursor: pageParam,
                limit: MAX_PAGE_SIZE,
            }),
        {
            enabled: !!id,
            getNextPageParam: ({ nextCursor, hasNextPage }) =>
                hasNextPage ? nextCursor : null,
        }
    );

    const observerElem = useRef<HTMLDivElement | null>(null);

    const handleObserver = useCallback(
        (entries: IntersectionObserverEntry[]) => {
            const [target] = entries;
            if (target.isIntersecting && hasNextPage && !isFetchingNextPage) {
                fetchNextPage();
            }
        },
        [fetchNextPage, hasNextPage, isFetchingNextPage]
    );

    useEffect(() => {
        const element = observerElem.current;
        if (!element) return;
        const option = { threshold: 0 };
        const observer = new IntersectionObserver(handleObserver, option);
        observer.observe(element);
        return () => observer.unobserve(element);
    }, [fetchNextPage, hasNextPage, handleObserver]);

    if (isInitialLoading) {
        return (
            <div className="mt-1 flex w-full justify-center">
                <LoadingSpinner />
            </div>
        );
    }

    // show the dynamic fields tab if there are pages and the first page has data
    const hasPages = !!data?.pages?.[0].data.length;

    return hasPages ? (
        <div className="mt-10">
            <TabGroup size="lg">
                <TabList>
                    <Tab>Dynamic Fields</Tab>
                </TabList>
                <TabPanels>
                    <TabPanel>
                        <div className="mt-4 flex max-h-[600px] flex-col gap-5 overflow-auto overflow-x-scroll">
                            {data.pages.map(({ data }) =>
                                // Show the field name and type is it is not an object
                                data.map((result) => (
                                    <DisclosureBox
                                        title={
                                            <div className="flex items-center gap-1 truncate break-words text-body font-medium leading-relaxed text-steel-dark">
                                                {typeof result.name?.value ===
                                                'object' ? (
                                                    <div className="block w-8/12 truncate break-words">
                                                        Struct{' '}
                                                        {result.objectType}
                                                    </div>
                                                ) : (
                                                    result.name?.value.toString()
                                                )}
                                                <ObjectLink
                                                    objectId={result.objectId}
                                                />
                                            </div>
                                        }
                                        variant="outline"
                                        key={result.objectId}
                                    >
                                        <div className="flex flex-col divide-y divide-gray-45">
                                            <UnderlyingObjectCard
                                                parentId={id}
                                                name={result.name}
                                            />
                                        </div>
                                    </DisclosureBox>
                                ))
                            )}

                            <div ref={observerElem}>
                                {isFetchingNextPage && hasNextPage ? (
                                    <div className="mt-1 flex w-full justify-center">
                                        <LoadingSpinner text="Loading data" />
                                    </div>
                                ) : null}
                            </div>
                        </div>
                    </TabPanel>
                </TabPanels>
            </TabGroup>
        </div>
    ) : null;
}
