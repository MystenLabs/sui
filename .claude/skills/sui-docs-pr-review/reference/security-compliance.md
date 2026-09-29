# Security Compliance for Documentation

All technical documentation must adhere to the [Sui Security Best Practices](https://docs.sui.io/develop/security/best-practices). This file defines how security best practices apply to documentation writing.

---

## Core rule

Every technical claim, code reference, architectural description, and recommendation in documentation must be consistent with Sui's published security best practices. Documentation must never recommend, demonstrate, or imply patterns that violate these practices.

## Banned patterns in documentation

Never write documentation that:

- Recommends or demonstrates using JSON-RPC for any purpose. JSON-RPC is deprecated. Use [GraphQL RPC](https://docs.sui.io/develop/graphql-rpc) or [gRPC](https://docs.sui.io/develop/grpc) instead.
- Shows `UpgradeCap` being transferred casually or stored in a single hot wallet without discussing governance ([source](https://docs.sui.io/develop/security/best-practices)).
- Relies solely on `tx_context::sender()` for access control without discussing the composability limitations ([source](https://docs.sui.io/develop/security/best-practices)).
- Demonstrates shared object access without authorization checks ([source](https://docs.sui.io/develop/security/best-practices)).
- Shows `TreasuryCap`, `MetadataCap`, or `DenyCapV2` without discussing their security implications ([source](https://docs.sui.io/develop/security/best-practices)).
- Accepts `RandomGenerator` as a function parameter instead of creating it internally ([source](https://docs.sui.io/develop/security/best-practices)).
- Recommends blind signing of opaque bytes ([source](https://docs.sui.io/develop/security/best-practices)).
- Trusts package IDs based on names alone without verification ([source](https://docs.sui.io/develop/security/best-practices)).
- Stores admin keys or capabilities in single hot wallets without discussing multisig or hardware custody ([source](https://docs.sui.io/develop/security/best-practices)).
- Creates capabilities without revocation paths ([source](https://docs.sui.io/develop/security/best-practices)).

## Required security context in documentation

When documenting the following topics, include the corresponding security guidance:

### Smart contracts
- Mention audit requirements for contracts handling user assets.
- Note that upgradeable dependencies can change after audit.
- Reference the need for event emission on privileged actions.

### Package upgrades
- Explain `UpgradeCap` governance: multisig, timelocks, or custom upgrade policies.
- Mention `make_immutable` when upgrades are no longer needed.
- Warn that this action is irreversible.

### Access control
- Prefer capability objects over allowlists as the default pattern.
- Document that every privileged function should require an explicit capability parameter.
- Note that shared objects need authorization enforcement because anyone can reference them.
- Include revocation path discussion for any capability pattern.

### Randomness
- Document that randomness-consuming functions must be private entry functions.
- Note the restriction on post-random commands.
- Mention commit-reveal for high-stakes applications.
- Warn against accepting `RandomGenerator` parameters.

### Coins and tokens
- Document the security implications of `TreasuryCap` (minting and burning control).
- Note `DenyCapV2` implications (can block addresses from transacting).
- Recommend setting supply model early.

### Seal (encryption)
- Document trusted operator selection and threshold encryption.
- Recommend envelope encryption (encrypt with your own key, use Seal for access management).
- Note that `seal_approve*` functions are security-critical.

### Nautilus (enclaves)
- Document minimal code requirements and dependency auditing.
- Mention PCR verification and short timestamps.
- Note that TEE guarantees alone are insufficient.

### Frontend and signing
- Require human-readable transaction intent display.
- Document package ID verification in the client.
- Mention domain separation for offchain messages.

### Key management
- Document multisig or hardware custody for admin keys.
- Recommend separating keys by role (deployer, admin, operations) and by network (Testnet, Mainnet).
- Include recovery procedure documentation requirements.

### Oracle and offchain data
- Require data freshness validation.
- Document source and signature verification.
- Define explicit failure behavior for missing or stale data.

## How to cite security practices

When documentation touches a security-relevant topic, cite the best practices page inline:

```mdx
Store your `UpgradeCap` in a multisig address or apply a custom upgrade policy.
For more information, see [Security Best Practices](https://docs.sui.io/develop/security/best-practices).
```

For specific warnings, use the appropriate admonition:

```mdx
:::danger

Never accept `RandomGenerator` as a function parameter. Always create generators
internally to prevent external manipulation.
See [Security Best Practices](https://docs.sui.io/develop/security/best-practices)
for the full randomness security model.

:::
```

## JSON-RPC ban

**JSON-RPC must never appear in any documentation produced by this skill.** This includes:

- API endpoint references (`https://fullnode.NETWORK.sui.io:443`)
- Method names (`sui_getObject`, `sui_executeTransactionBlock`, and so on)
- Example requests or responses using JSON-RPC format
- Links to JSON-RPC reference documentation
- Recommendations to use JSON-RPC for any purpose

If existing documentation or source material contains JSON-RPC references, replace them with the equivalent GraphQL RPC or gRPC approach. If no equivalent exists yet, insert a placeholder:

```mdx
{/* TODO: Replace JSON-RPC reference with GraphQL RPC or gRPC equivalent */}
```

### Approved API references

| API | When to use | Documentation |
|---|---|---|
| GraphQL RPC | Querying onchain state, transaction history, object data | [GraphQL RPC docs](https://docs.sui.io/develop/graphql-rpc) |
| gRPC | High-throughput streaming, indexing pipelines, real-time subscriptions | [gRPC docs](https://docs.sui.io/develop/grpc) |
