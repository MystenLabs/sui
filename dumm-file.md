# Persisting client state
# TLDR: How do we take advantage of storage to aid proper client reloads?
# Do we persist all old object?

The following data structures from  https://github.com/MystenLabs/fastnft/blob/main/fastpay_core/src/client.rs#L59 will be modified and represented in some `store: Arc<ClientStore>,` similar on AuthorityStore https://github.com/MystenLabs/fastnft/blob/main/fastpay_core/src/authority/authority_store.rs
```rust
pub struct ClientState<AuthorityClient> {
    /// Our FastPay address.
    address: FastPayAddress,
    /// Our signature key.
    secret: KeyPair,
    /// Our FastPay committee.
    committee: Committee,
    /// How to talk to this committee.
    authority_clients: HashMap<AuthorityName, AuthorityClient>,
    /// Pending transfer.
    pending_transfer: Option<Order>,

    // The remaining fields are used to minimize networking, and may not always be persisted locally.
    /// Known certificates, indexed by TX digest.
    certificates: BTreeMap<TransactionDigest, CertifiedOrder>,
    /// The known objects with it's sequence number owned by the client.
    object_sequence_numbers: BTreeMap<ObjectID, SequenceNumber>,
    /// Confirmed objects with it's ref owned by the client.
    object_refs: BTreeMap<ObjectID, ObjectRef>,
    /// Certificate <-> object id linking map.
    object_certs: BTreeMap<ObjectID, Vec<TransactionDigest>>,
}
```
However with some changes

The following will be kept and persisted normally (as DBMap):
```rust
    address: FastPayAddress,
    secret: KeyPair,
    committee: Committee,
    authority_clients: HashMap<AuthorityName, AuthorityClient>,
    certificates: BTreeMap<TransactionDigest, CertifiedOrder>,
    object_refs: BTreeMap<ObjectID, ObjectRef>,
    object_certs: BTreeMap<ObjectID, Vec<TransactionDigest>>,
```

The following will be deleted:

 `object_sequence_numbers: BTreeMap<ObjectID, SequenceNumber>` as it can be derived from `object_refs: BTreeMap<ObjectID, ObjectRef>`

The following will be added:
 
 `past_objects_map: DBMap<ObjectID, BTreeMap<SequenceNumber, Object>>` which stores all versions of the old objects

`latest_object_map: DBMap<ObjectID, Object>` which stores the latest object (for lightweight retrieval)





**Tricky part:**

`pending_transfer: Option<Order>` should be changed to `pending_orders: HashMap<ObjectID, Order>` and should be persisted to prevent equivocation on crash & restart

*What's the proper way of saving and reloading pending orders?*

My thought process:

Upon every tx order, the client saves the ObjectId-Order pair to the `pending_transfer` map and clears it when finalized. While a flag is set for an object, the client cannot mutate the object. If the client crashes before clearing the flag, it can replay the order after restarting. This is safe due to idempotence. 
