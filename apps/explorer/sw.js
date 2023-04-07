// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

const cacheName = 'site-cache-v1';
const assetsToCache = ['index.html'];

self.addEventListener('install', (event) => {
    event.waitUntil(
        caches.open(cacheName).then((cache) => {
            return cache.addAll(assetsToCache);
        })
    );
});

self.addEventListener('fetch', (event) => {});
