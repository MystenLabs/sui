// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0
import { Button } from "@radix-ui/themes";
import { ReactNode, useEffect, useRef } from "react";
import { Loading } from "./Loading";

/**
 * An infinite scroll area that calls `loadMore()` when the user scrolls to the bottom.
 * Helps build easy infinite scroll areas for paginated data.
 */
export function InfiniteScrollArea({
  children,
  loadMore,
  loading = false,
  hasNextPage,
  gridClasses = "py-6 grid-cols-1 md:grid-cols-2 gap-5",
}: {
  children: ReactNode | ReactNode[];
  loadMore: () => void;
  loading: boolean;
  hasNextPage: boolean;
  gridClasses?: string;
}) {
  const observerTarget = useRef(null);

  // implement infinite loading.
  useEffect(() => {
    const observer = new IntersectionObserver(
      (entries) => {
        if (entries[0].isIntersecting) {
          loadMore();
        }
      },
      { threshold: 1 },
    );

    if (observerTarget.current) {
      observer.observe(observerTarget.current);
    }

    return () => {
      if (observerTarget.current) {
        // eslint-disable-next-line react-hooks/exhaustive-deps
        observer.unobserve(observerTarget.current);
      }
    };
  }, [observerTarget, loadMore]);

  if (!children || (Array.isArray(children) && children.length === 0))
    return <div className="p-3">No results found.</div>;
  return (
    <>
      <div className={`grid ${gridClasses}`}>{children}</div>

      <div className="col-span-2 text-center">
        {loading && <Loading />}

        {hasNextPage && !loading && (
          <Button
            ref={observerTarget}
            color="gray"
            className="cursor-pointer"
            onClick={loadMore}
            disabled={!hasNextPage || loading}
          >
            Load more...
          </Button>
        )}
      </div>
    </>
  );
}
