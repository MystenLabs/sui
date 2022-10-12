// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0
use futures::{
    stream::{FuturesOrdered, FuturesUnordered},
    Future, StreamExt, TryFutureExt, TryStreamExt,
};
use tokio::sync::{AcquireError, Semaphore, SemaphorePermit};

pub struct UnorderedPermit<'a, T: Future> {
    permit: SemaphorePermit<'a>,
    futures: &'a BoundedFuturesUnordered<T>,
}

/// An async-friendly FuturesUnordered of bounded size. In contrast to a bounded channel,
/// the queue makes it possible to modify and remove entries in it. In contrast to a FuturesUnordered,
/// the queue makes it possible to enforce a bound on the number of items in the queue.
pub struct BoundedFuturesUnordered<T: Future> {
    /// The maximum number of entries allowed in the queue
    capacity: usize,
    /// The actual items in the queue.
    queue: FuturesUnordered<T>,
    /// This semaphore has as many permits as there are empty spots in the
    /// `queue`, i.e., `capacity - queue.len()` many permits
    push_semaphore: Semaphore,
}

impl<T: Future> std::fmt::Debug for BoundedFuturesUnordered<T> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "BoundedFuturesUnordered[cap: {}, free: {}]",
            self.capacity,
            self.push_semaphore.available_permits(),
        )
    }
}

unsafe impl<T: Future> Sync for BoundedFuturesUnordered<T> {}
unsafe impl<T: Future> Send for BoundedFuturesUnordered<T> {}

// We expect to grow this facade over time
impl<T: Future> BoundedFuturesUnordered<T> {
    pub fn with_capacity(capacity: usize) -> Self {
        Self {
            capacity,
            queue: FuturesUnordered::new(),
            push_semaphore: Semaphore::new(capacity),
        }
    }

    /// Push an item into the queue. If the queue is currently full this method
    /// blocks until an item is available
    pub async fn push(&self, item: T) {
        let permit = self.push_semaphore.acquire().await.unwrap();
        self.queue.push(item);
        permit.forget();
    }

    pub fn push_with_permit(&self, item: T, _permit: SemaphorePermit<'_>) {
        self.queue.push(item);
    }

    /// Waits for queue capacity. Once capacity to push one future is available, it is reserved for the caller.
    ///
    /// WARNING: the order of pushing to the queue is not guaranteed. It must be enforced by the caller.
    pub async fn reserve(&self) -> Result<UnorderedPermit<'_, T>, AcquireError> {
        let permit = self.push_semaphore.acquire().await?;
        Ok(UnorderedPermit {
            permit,
            futures: self,
        })
    }

    /// Report the available permits
    pub fn available_permits(&self) -> usize {
        self.push_semaphore.available_permits()
    }

    /// Report the length of the queue
    pub fn len(&self) -> usize {
        self.queue.len()
    }

    /// Report if  the queue is empty
    pub fn is_empty(&self) -> bool {
        self.queue.is_empty()
    }
}

impl<U, V, T: Future<Output = Result<U, V>>> BoundedFuturesUnordered<T> {
    /// Creates a future that attempts to resolve the next item in the stream.
    /// If an error is encountered before the next item, the error is returned instead.
    pub fn try_next(&mut self) -> impl Future<Output = Result<Option<U>, V>> + '_ {
        self.queue.try_next().inspect_ok(|val| {
            if val.is_some() {
                self.push_semaphore.add_permits(1)
            }
        })
    }
}

impl<T: Future> BoundedFuturesUnordered<T> {
    pub async fn next(&mut self) -> Option<T::Output> {
        let result = self.queue.next().await;
        self.push_semaphore.add_permits(1);
        result
    }
}

impl<'a, T: Future> UnorderedPermit<'a, T> {
    /// Push an item using the reserved permit
    pub fn push(self, item: T) {
        self.futures.push_with_permit(item, self.permit);
    }
}

/// An async-friendly FuturesUnordered of bounded size. In contrast to a bounded channel,
/// the queue makes it possible to modify and remove entries in it. In contrast to a FuturesUnordered,
/// the queue makes it possible to enforce a bound on the number of items in the queue.
pub struct BoundedFuturesOrdered<T: Future> {
    /// The maximum number of entries allowed in the queue
    capacity: usize,
    /// The actual items in the queue. New items are appended at the back.
    queue: FuturesOrdered<T>,
    /// This semaphore has as many permits as there are empty spots in the
    /// `queue`, i.e., `capacity - queue.len()` many permits
    push_semaphore: Semaphore,
}

impl<T: Future> std::fmt::Debug for BoundedFuturesOrdered<T> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "BoundedFuturesUnordered[cap: {}, queue: {}, push: {}]",
            self.capacity,
            self.queue.len(),
            self.push_semaphore.available_permits(),
        )
    }
}

unsafe impl<T: Future> Sync for BoundedFuturesOrdered<T> {}
unsafe impl<T: Future> Send for BoundedFuturesOrdered<T> {}

// We expect to grow this facade over time
impl<T: Future> BoundedFuturesOrdered<T> {
    pub fn with_capacity(capacity: usize) -> Self {
        Self {
            capacity,
            queue: FuturesOrdered::new(),
            push_semaphore: Semaphore::new(capacity),
        }
    }

    /// Push an item into the queue. If the queue is currently full this method
    /// blocks until an item is available
    pub async fn push(&mut self, item: T) {
        let permit = self.push_semaphore.acquire().await.unwrap();
        self.queue.push_back(item);
        permit.forget();
    }

    /// Report the available permits
    pub fn available_permits(&self) -> usize {
        self.push_semaphore.available_permits()
    }

    /// Report the length of the queue
    pub fn len(&self) -> usize {
        self.queue.len()
    }

    /// Report if  the queue is empty
    pub fn is_empty(&self) -> bool {
        self.queue.is_empty()
    }
}

impl<U, V, T: Future<Output = Result<U, V>>> BoundedFuturesOrdered<T> {
    /// Creates a future that attempts to resolve the next item in the stream.
    /// If an error is encountered before the next item, the error is returned instead.
    pub fn try_next(&mut self) -> impl Future<Output = Result<Option<U>, V>> + '_ {
        self.queue.try_next().inspect_ok(|val| {
            if val.is_some() {
                self.push_semaphore.add_permits(1)
            }
        })
    }
}

#[cfg(test)]
mod tests {

    use super::{BoundedFuturesOrdered, BoundedFuturesUnordered};
    use futures::{future, FutureExt};

    #[tokio::test]
    async fn test_capacity_up() {
        let cap = 10;
        let futs = BoundedFuturesUnordered::with_capacity(cap);
        for i in 0..cap {
            futs.push(future::ready(i)).await;
            assert_eq!(futs.push_semaphore.available_permits(), cap - i - 1);
        }
        assert!(futs.push(future::ready(10)).now_or_never().is_none());
    }

    #[tokio::test]
    async fn test_reserve_up() {
        let cap = 10;
        let futs = BoundedFuturesUnordered::with_capacity(cap);
        let mut permits = Vec::new();
        // this forces the type of the future
        futs.push(future::ready(0)).await;
        for i in 1..cap {
            let permit = futs.reserve().await.unwrap();
            permits.push(permit);
            assert_eq!(futs.push_semaphore.available_permits(), cap - i - 1);
        }
        assert_eq!(futs.push_semaphore.available_permits(), 0);
        assert!(futs.push(future::ready(1)).now_or_never().is_none());
        drop(permits);
        assert_eq!(futs.push_semaphore.available_permits(), 9);
    }

    #[tokio::test]
    async fn test_reserve_down() {
        let cap = 10;
        let futs = BoundedFuturesUnordered::with_capacity(cap);
        let mut permits = Vec::new();
        // this forces the type of the future
        futs.push(future::ready(0)).await;
        for i in 1..cap {
            let permit = futs.reserve().await.unwrap();
            permits.push(permit);
            assert_eq!(futs.push_semaphore.available_permits(), cap - i - 1);
        }
        assert_eq!(futs.push_semaphore.available_permits(), 0);
        assert!(futs.push(future::ready(1)).now_or_never().is_none());
        for (i, permit) in permits.into_iter().enumerate() {
            permit.push(future::ready(i));
        }
        assert_eq!(futs.push_semaphore.available_permits(), 9);
    }

    #[tokio::test]
    async fn test_capacity_down() {
        let cap = 10;
        let mut futs = BoundedFuturesUnordered::with_capacity(cap);

        for i in 0..10 {
            futs.push(future::ready(Result::<usize, bool>::Ok(i))).await
        }
        for i in 0..10 {
            assert!(futs.try_next().await.unwrap().is_some());
            assert_eq!(futs.push_semaphore.available_permits(), i + 1)
        }
        assert!(futs.try_next().await.unwrap().is_none());
        assert_eq!(futs.push_semaphore.available_permits(), cap)
    }

    #[tokio::test]
    async fn test_capacity_up_ordered() {
        let cap = 10;
        let mut futs = BoundedFuturesOrdered::with_capacity(cap);
        for i in 0..cap {
            futs.push(future::ready(i)).await;
            assert_eq!(futs.push_semaphore.available_permits(), cap - i - 1);
        }
        assert!(futs.push(future::ready(10)).now_or_never().is_none());
    }

    #[tokio::test]
    async fn test_capacity_down_ordered() {
        let cap = 10;
        let mut futs = BoundedFuturesOrdered::with_capacity(cap);

        for i in 0..10 {
            futs.push(future::ready(Result::<usize, bool>::Ok(i))).await
        }
        for i in 0..10 {
            assert!(futs.try_next().await.unwrap().is_some());
            assert_eq!(futs.push_semaphore.available_permits(), i + 1)
        }
        assert!(futs.try_next().await.unwrap().is_none());
        assert_eq!(futs.push_semaphore.available_permits(), cap)
    }
}
