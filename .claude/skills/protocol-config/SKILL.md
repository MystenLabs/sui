# Modify Protocol Config

Guides you through safely modifying the Sui protocol configuration.

## Usage

```
/protocol-config
```

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

**Add a new feature flag:**
```rust
cfg.feature_flags.new_feature = true;
```

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

**Enable on mainnet only:**
```rust
if chain == Chain::Mainnet {
    cfg.feature_flags.some_mainnet_specific_thing = true;
}
```

**Enable everywhere (no chain check needed):**
```rust
cfg.feature_flags.stable_feature = true;
```

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

| Network | Chain Value | Pattern |
|---------|-------------|---------|
| Devnet | `Chain::Unknown` | `chain != Chain::Mainnet && chain != Chain::Testnet` |
| Testnet | `Chain::Testnet` | `chain != Chain::Mainnet` |
| Mainnet | `Chain::Mainnet` | `chain == Chain::Mainnet` or no condition |

## File Locations

- Protocol config: `crates/sui-protocol-config/src/lib.rs`
- Snapshots: `crates/sui-protocol-config/src/snapshots/`
