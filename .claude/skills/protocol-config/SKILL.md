# Modify Protocol Config

Guides you through safely modifying the Sui protocol configuration, or verifies that existing changes were made correctly.

## Usage

```
/protocol-config
```

## When to Use

This skill serves two purposes:

1. **Making changes** - Use this skill to guide you through adding new features or modifying protocol config settings.

2. **Verifying changes** - Use this skill to check that protocol config changes already made in a branch are correct. This includes verifying that:
   - A new protocol version was created if needed (not modifying a released version)
   - The version history comment was added
   - Chain-specific guards are used appropriately
   - Snapshots were updated

When verifying existing work, follow steps 1-4 to confirm the changes are safe, then review the actual changes against the patterns in steps 5-7.

## Background

The protocol config is defined in `crates/sui-protocol-config/src/lib.rs`. It controls blockchain behavior across all Sui networks. The critical constraint is:

**Once a protocol version has been released to a production network, its settings CANNOT be changed.**

This means if mainnet or testnet is running protocol version N, the settings for version N (and all prior versions) are frozen forever.

## Instructions

### 1. Determine the most recently cut release branch

Find the latest release branch:

```bash
git fetch origin
git branch -r | grep 'origin/releases/sui-v' | grep -E '\-release$' | sort -V | tail -1
```

This returns something like `origin/releases/sui-v1.66.0-release`.

### 2. Check the max protocol version in the release branch

```bash
git show <release-branch>:crates/sui-protocol-config/src/lib.rs | grep 'const MAX_PROTOCOL_VERSION'
```

For example:
```bash
git show origin/releases/sui-v1.66.0-release:crates/sui-protocol-config/src/lib.rs | grep 'const MAX_PROTOCOL_VERSION'
```

### 3. Check the max protocol version in your feature branch

```bash
grep 'const MAX_PROTOCOL_VERSION' crates/sui-protocol-config/src/lib.rs
```

### 4. Determine if you need a new protocol version

**If the release branch and your feature branch have the SAME max version number**, you MUST create a new protocol version in your feature branch before making changes.

**If your feature branch already has a HIGHER max version number**, you can add changes to the existing version block.

### 5. Creating a new protocol version

If you need a new version:

1. **Increment MAX_PROTOCOL_VERSION** at the top of the file (around line 27):
   ```rust
   const MAX_PROTOCOL_VERSION: u64 = 113;  // was 112
   ```

2. **Add a version history comment** (around line 299, after the last version comment):
   ```rust
   // Version 113: <Brief description of your changes>
   ```

3. **Add a new version match block** in `get_for_version_impl()` (search for the last numbered version block, currently around line 4575):
   ```rust
   113 => {
       // Your changes here
   }
   ```

### 6. Making your changes

Add your changes to the appropriate version block. Follow these patterns:

**Modify an existing constant:**
```rust
cfg.some_constant = Some(new_value);
```

**Add a new constant (which is None in prior versions):**
```rust
cfg.new_constant = Some(new_value);
```
Note: You must also add the field to the `ProtocolConfig` struct with type `Option<T>`. The `ProtocolConfigAccessors` derive macro automatically generates accessor methods (`new_constant()` and `new_constant_as_option()`).

**Add a new feature flag:**
```rust
cfg.feature_flags.new_feature = true;
```
Note: You must also add the field to the `FeatureFlags` struct with type `bool`. The `ProtocolConfigFeatureFlagsGetters` derive macro automatically generates the getter method.

### 7. Using Chain for network-specific features

Use the `chain` parameter to enable features on specific networks. Features typically roll out progressively: devnet -> testnet -> mainnet.

**Enable on devnet only (Chain::Unknown):**
```rust
if chain != Chain::Mainnet && chain != Chain::Testnet {
    cfg.feature_flags.new_feature = true;
}
```

**Enable on devnet and testnet:**
```rust
if chain != Chain::Mainnet {
    cfg.feature_flags.new_feature = true;
}
```

**Enable everywhere (preferred for new features going to mainnet):**
```rust
cfg.feature_flags.stable_feature = true;
```
This is the preferred pattern when enabling a feature for mainnet. Use unconditional enabling rather than `chain == Chain::Mainnet`.

**Enable on mainnet only (rare - excludes devnet/testnet):**
```rust
if chain == Chain::Mainnet {
    cfg.feature_flags.some_mainnet_specific_thing = true;
}
```
This pattern is rarely needed. It enables a feature ONLY on mainnet while keeping it disabled on devnet and testnet. Only use this for mainnet-specific behavior that should not exist on other networks.

### 8. Update snapshots

After making changes, update the protocol config snapshots:

```bash
cargo insta test -p sui-protocol-config --accept
```

### 9. Common mistakes to avoid

- **Never modify settings for a version that exists in a release branch** - this will break consensus
- **Always add new feature flags to FeatureFlags struct** if they don't exist
- **Always add new config values to ProtocolConfig struct** if they don't exist
- **Remember to update version history comments** - they serve as documentation

## Quick Reference

| Target Networks | Pattern |
|-----------------|---------|
| Devnet only | `if chain != Chain::Mainnet && chain != Chain::Testnet` |
| Devnet + Testnet | `if chain != Chain::Mainnet` |
| All networks (including Mainnet) | No condition (preferred) |
| Mainnet only (rare) | `if chain == Chain::Mainnet` |

## File Locations

- Protocol config: `crates/sui-protocol-config/src/lib.rs`
- Snapshots: `crates/sui-protocol-config/src/snapshots/`
