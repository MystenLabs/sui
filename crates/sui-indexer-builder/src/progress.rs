// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use std::collections::{HashMap, HashSet};
use std::sync::{Arc, Mutex};

#[derive(Debug, Clone)]
pub enum ProgressSavingPolicy {
    SaveAfterDuration(SaveAfterDurationPolicy),
    OutOfOrderSaveAfterDuration(OutOfOrderSaveAfterDurationPolicy),
}

#[derive(Debug, Clone)]
pub struct SaveAfterDurationPolicy {
    duration: tokio::time::Duration,
    last_save_time: Arc<Mutex<HashMap<String, Option<tokio::time::Instant>>>>,
}

impl SaveAfterDurationPolicy {
    pub fn new(duration: tokio::time::Duration) -> Self {
        Self {
            duration,
            last_save_time: Arc::new(Mutex::new(HashMap::new())),
        }
    }
}

#[derive(Debug, Clone)]
pub struct OutOfOrderSaveAfterDurationPolicy {
    duration: tokio::time::Duration,
    last_save_time: Arc<Mutex<HashMap<String, Option<tokio::time::Instant>>>>,
    seen: Arc<Mutex<HashMap<String, HashSet<u64>>>>,
    next_to_fill: Arc<Mutex<HashMap<String, Option<u64>>>>,
}

impl OutOfOrderSaveAfterDurationPolicy {
    pub fn new(duration: tokio::time::Duration) -> Self {
        Self {
            duration,
            last_save_time: Arc::new(Mutex::new(HashMap::new())),
            seen: Arc::new(Mutex::new(HashMap::new())),
            next_to_fill: Arc::new(Mutex::new(HashMap::new())),
        }
    }
}

impl ProgressSavingPolicy {
    /// If returns Some(progress), it means we should save the progress to DB.
    pub fn cache_progress(
        &mut self,
        task_name: String,
        heights: &[u64],
        start_height: u64,
        target_height: u64,
    ) -> Option<u64> {
        match self {
            ProgressSavingPolicy::SaveAfterDuration(policy) => {
                let height = *heights.iter().max().unwrap();
                let mut last_save_time_guard = policy.last_save_time.lock().unwrap();
                let last_save_time = last_save_time_guard.entry(task_name).or_insert(None);
                if height >= target_height {
                    *last_save_time = Some(tokio::time::Instant::now());
                    return Some(height);
                }
                if let Some(v) = last_save_time {
                    if v.elapsed() >= policy.duration {
                        *last_save_time = Some(tokio::time::Instant::now());
                        Some(height)
                    } else {
                        None
                    }
                } else {
                    // update `last_save_time` to now but don't actually save progress
                    *last_save_time = Some(tokio::time::Instant::now());
                    None
                }
            }
            ProgressSavingPolicy::OutOfOrderSaveAfterDuration(policy) => {
                let mut next_to_fill = {
                    let mut next_to_fill_guard = policy.next_to_fill.lock().unwrap();
                    (*next_to_fill_guard
                        .entry(task_name.clone())
                        .or_insert(Some(start_height)))
                    .unwrap()
                };
                let old_next_to_fill = next_to_fill;
                {
                    let mut seen_guard = policy.seen.lock().unwrap();
                    let seen = seen_guard
                        .entry(task_name.clone())
                        .or_insert(HashSet::new());
                    seen.extend(heights.iter().cloned());
                    while seen.remove(&next_to_fill) {
                        next_to_fill += 1;
                    }
                }
                // We made some progress in filling gaps
                if old_next_to_fill != next_to_fill {
                    policy
                        .next_to_fill
                        .lock()
                        .unwrap()
                        .insert(task_name.clone(), Some(next_to_fill));
                }

                let mut last_save_time_guard = policy.last_save_time.lock().unwrap();
                let last_save_time = last_save_time_guard
                    .entry(task_name.clone())
                    .or_insert(None);

                // If we have reached the target height, we always save
                if next_to_fill > target_height {
                    *last_save_time = Some(tokio::time::Instant::now());
                    return Some(next_to_fill - 1);
                }
                // Regardless of whether we made progress, we should save if we have waited long enough
                if let Some(v) = last_save_time {
                    if v.elapsed() >= policy.duration && next_to_fill > start_height {
                        *last_save_time = Some(tokio::time::Instant::now());
                        Some(next_to_fill - 1)
                    } else {
                        None
                    }
                } else {
                    // update `last_save_time` to now but don't actually save progress
                    *last_save_time = Some(tokio::time::Instant::now());
                    None
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {

    use super::*;

    #[tokio::test]
    async fn test_save_after_duration_policy() {
        let duration = tokio::time::Duration::from_millis(100);
        let mut policy =
            ProgressSavingPolicy::SaveAfterDuration(SaveAfterDurationPolicy::new(duration));
        assert_eq!(
            policy.cache_progress("task1".to_string(), &[1], 0, 100),
            None
        );
        tokio::time::sleep(duration).await;
        assert_eq!(
            policy.cache_progress("task1".to_string(), &[2], 0, 100),
            Some(2)
        );
        tokio::time::sleep(duration).await;
        assert_eq!(
            policy.cache_progress("task1".to_string(), &[3], 0, 100),
            Some(3)
        );

        assert_eq!(
            policy.cache_progress("task2".to_string(), &[4], 0, 100),
            None
        );
        tokio::time::sleep(duration).await;
        assert_eq!(
            policy.cache_progress("task2".to_string(), &[5, 6], 0, 100),
            Some(6)
        );
        tokio::time::sleep(duration).await;
        assert_eq!(
            policy.cache_progress("task2".to_string(), &[8, 7], 0, 100),
            Some(8)
        );
    }

    #[tokio::test]
    async fn test_out_of_order_save_after_duration_policy() {
        let duration = tokio::time::Duration::from_millis(100);
        let mut policy = ProgressSavingPolicy::OutOfOrderSaveAfterDuration(
            OutOfOrderSaveAfterDurationPolicy::new(duration),
        );

        assert_eq!(
            policy.cache_progress("task1".to_string(), &[0], 0, 100),
            None
        );
        tokio::time::sleep(duration).await;
        assert_eq!(
            policy.cache_progress("task1".to_string(), &[1], 0, 100),
            Some(1)
        );
        assert_eq!(
            policy.cache_progress("task1".to_string(), &[3], 0, 100),
            None
        );
        tokio::time::sleep(duration).await;
        assert_eq!(
            policy.cache_progress("task1".to_string(), &[4], 0, 100),
            Some(1)
        );
        tokio::time::sleep(duration).await;
        assert_eq!(
            policy.cache_progress("task1".to_string(), &[2], 0, 100),
            Some(4)
        );

        assert_eq!(
            policy.cache_progress("task2".to_string(), &[0], 0, 100),
            None
        );
        tokio::time::sleep(duration).await;
        assert_eq!(
            policy.cache_progress("task2".to_string(), &[1], 0, 100),
            Some(1)
        );
        tokio::time::sleep(duration).await;
        assert_eq!(
            policy.cache_progress("task2".to_string(), &[2], 0, 100),
            Some(2)
        );
        assert_eq!(
            policy.cache_progress("task2".to_string(), &[3], 0, 100),
            None
        );
        tokio::time::sleep(duration).await;
        assert_eq!(
            policy.cache_progress("task2".to_string(), &[4], 0, 100),
            Some(4)
        );

        assert_eq!(
            policy.cache_progress("task2".to_string(), &[6, 7, 8], 0, 100),
            None
        );
        tokio::time::sleep(duration).await;
        assert_eq!(
            policy.cache_progress("task2".to_string(), &[5, 9], 0, 100),
            Some(9)
        );
    }
}
