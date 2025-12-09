We need to add logic to be able to download missing objects when forked from a network at a specific checkpoint. That   means, we will need to have a logic that will
   - deconstruct the transaction to find which objects are needed to execute the transaction
   - check if those objects are available in the local cache. The local cache is the one used in `../sui-replay-2` which can be a combination of file-based cache, in-memory-cache, or others. We should reuse code from there. If objects are found in the cache, then we can execute the transaction. If not, we will need to download missing objects
   - download the missing objects from the network at that checkpoint. If no checkpoint is given, then at initialization we will need to figure out the latest checkpoint at that moment and store that. Once objects are downloaded, they need to be added to the local cache.
   - once we have all the objects, we can execute the transaction
