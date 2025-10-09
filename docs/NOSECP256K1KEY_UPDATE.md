# NoSecp256k1Key Type Update for P2TSH Example

## Overview

Updated the `examples/p2tsh.rs` to use the new `NoSecp256k1Key` type instead of `XOnlyPublicKey` for SLH-DSA post-quantum miniscripts. This change makes the code more self-documenting and semantically correct.

## Changes Made

### 1. Updated Imports (`examples/p2tsh.rs` line 40)

**Before:**
```rust
use bdk_wallet::miniscript::{Miniscript, Tap, Satisfier, MiniscriptKey, ToPublicKey};
```

**After:**
```rust
use bdk_wallet::miniscript::{Miniscript, Tap, Satisfier, MiniscriptKey, ToPublicKey, NoSecp256k1Key};
```

### 2. Updated Miniscript Type (`examples/p2tsh.rs` lines 188-196)

**Before:**
```rust
// Create miniscript with SLH-DSA terminal
// This creates: <32-byte-key> OP_SUCCESS127 (0x7f)
let slh_dsa_ms: Miniscript<XOnlyPublicKey, Tap> = Miniscript::slh_dsa_pk(slh_dsa_key);
```

**After:**
```rust
// Create a Miniscript that compiles to: <32-byte-slh-dsa-key> OP_SUCCESS127
//
// Note: NoSecp256k1Key is a placeholder type parameter for Miniscript<Pk, Ctx>
// - The slh_dsa_pk() method uses the concrete SlhDsaPublicKey type internally
// - NoSecp256k1Key satisfies the MiniscriptKey trait requirement without providing
//   actual secp256k1 key functionality
// - This makes the code self-documenting: it clearly indicates this miniscript
//   contains only post-quantum keys, no secp256k1 keys
let slh_dsa_ms: Miniscript<NoSecp256k1Key, Tap> = Miniscript::slh_dsa_pk(slh_dsa_key);
```

### 3. Updated Tsh Descriptor Type (`examples/p2tsh.rs` lines 204-206)

**Before:**
```rust
let tsh: Tsh<XOnlyPublicKey> = Tsh::new(Some(tap_tree.clone()))
    .expect("Failed to create Tsh descriptor");
```

**After:**
```rust
// Create P2TSH descriptor using miniscript
// Note: NoSecp256k1Key is used to indicate this descriptor contains only post-quantum keys
let tsh: Tsh<NoSecp256k1Key> = Tsh::new(Some(tap_tree.clone()))
    .expect("Failed to create Tsh descriptor");
```

### 4. Added NoSecp256k1Key Export (`src/descriptor/mod.rs` line 32)

**Before:**
```rust
pub use miniscript::{
    Descriptor, DescriptorPublicKey, Legacy, Miniscript, ScriptContext, Segwitv0,
};
```

**After:**
```rust
pub use miniscript::{
    Descriptor, DescriptorPublicKey, Legacy, Miniscript, NoSecp256k1Key, ScriptContext, Segwitv0,
};
```

## Why This Change?

### The Problem with `XOnlyPublicKey`

The original code used `XOnlyPublicKey` as the type parameter:
- **Confusing**: Suggests Schnorr/x-only public keys are used
- **Misleading**: SLH-DSA scripts don't use secp256k1 keys at all
- **Semantically incorrect**: The type parameter implies traditional cryptography

### The Solution: `NoSecp256k1Key`

The new `NoSecp256k1Key` type:
- **Self-documenting**: Clearly indicates no secp256k1 keys are present
- **Type-safe**: Implements `MiniscriptKey` trait but marks key methods as `unreachable!()`
- **Semantically correct**: Accurately represents post-quantum-only miniscripts

## Technical Details

### What is `NoSecp256k1Key`?

From `rust-miniscript/src/lib.rs:233-263`:

```rust
/// Placeholder key type for miniscripts that only contain non-secp256k1 keys.
///
/// This type is used as a generic parameter for `Miniscript<Pk, Ctx>` when the
/// miniscript contains only post-quantum or other non-secp256k1 keys (like SLH-DSA).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct NoSecp256k1Key;

impl MiniscriptKey for NoSecp256k1Key {
    type Sha256 = sha256::Hash;
    type Hash256 = hash256::Hash;
    type Ripemd160 = ripemd160::Hash;
    type Hash160 = hash160::Hash;
}

impl ToPublicKey for NoSecp256k1Key {
    /// This should never be called - NoSecp256k1Key is only a placeholder
    fn to_public_key(&self) -> bitcoin::PublicKey {
        unreachable!("NoSecp256k1Key::to_public_key() should never be called")
    }
    // ... other methods also unreachable!()
}
```

### Why Does It Work?

1. **SLH-DSA uses concrete types**: The `slh_dsa_pk()` method uses `SlhDsaPublicKey` internally, not the generic `Pk` parameter
2. **Type system requirement**: `Miniscript<Pk, Ctx>` requires a `Pk: MiniscriptKey`, even if unused
3. **Safety**: The `unreachable!()` methods ensure errors if accidentally called

## P2TSH Context Notes

### Important P2TSH Characteristics

1. **SegWit Version 2**: P2TSH is SegWit v2 (not v1 like P2TR)
2. **No Key-Path Spending**: Unlike P2TR, P2TSH has NO key-path spend - script-path only
3. **No Internal Key**: P2TSH doesn't have an internal/output key to tweak

### Why `Tap` Context is Still Correct

- **`Tap` context**: Defines **TapScript** language rules (opcodes, validation)
- **Used by both**: P2TR (SegWit v1) and P2TSH (SegWit v2) both use TapScript
- **Not about output type**: The context is about the script language, not the output format

## Verification

### Build and Run

```bash
$ cargo check --example p2tsh
    Finished `dev` profile [unoptimized + debuginfo] target(s) in 0.14s

$ cargo run --example p2tsh
✓ Created SLH-DSA miniscript: slh_dsa_pk(cef4d3...)
✓ Created TapTree with SLH-DSA leaf
✓ P2TSH Descriptor: tsh(slh_dsa_pk(cef4d3...))#mr5q4w23
✓ P2TSH Address: bc1zsg0zv8ghg8sa8wezw9sjr7h56tcdqy6d7x08v4wrwkg84xtme5vq9ujzxg
```

### Type Signatures

```rust
// Pure post-quantum miniscript
let ms: Miniscript<NoSecp256k1Key, Tap> = Miniscript::slh_dsa_pk(slh_key);

// Pure post-quantum descriptor  
let tsh: Tsh<NoSecp256k1Key> = Tsh::new(Some(TapTree::leaf(ms)))?;

// Display clearly shows SLH-DSA
println!("{}", tsh); // tsh(slh_dsa_pk(...))
```

## Benefits

### 1. Code Clarity
```rust
// Before: Confusing - why XOnlyPublicKey for SLH-DSA?
let ms: Miniscript<XOnlyPublicKey, Tap> = Miniscript::slh_dsa_pk(key);

// After: Clear - this is post-quantum only!
let ms: Miniscript<NoSecp256k1Key, Tap> = Miniscript::slh_dsa_pk(key);
```

### 2. Type Safety
- Compiler prevents mixing incompatible key types in some contexts
- `unreachable!()` guards catch programming errors at runtime

### 3. Documentation
- Type signature itself documents the intent
- No need to explain "why XOnlyPublicKey when we're not using it"

## Related Documentation

- **Upstream miniscript**: `rust-miniscript/doc/SLH_DSA_INTEGRATION.md` 
- **BDK integration**: `bdk/docs/MINISCRIPT_13.0.0-pqc-0.2_UPDATE.md`
- **SLH-DSA in BDK**: `bdk/docs/SLH_DSA_INTEGRATION.md`

## Version Requirements

- **miniscript**: v13.0.0-pqc-0.2+ (includes `NoSecp256k1Key`)
- **bdk_chain**: v0.23.2-pqc-0.1+ (compatible with new miniscript)
- **bdk_wallet**: v3.0.0-alpha.0-pqc-0.0 (this workspace)

## Summary

The update from `XOnlyPublicKey` to `NoSecp256k1Key` makes the P2TSH example:
- ✅ More semantically correct
- ✅ Self-documenting
- ✅ Less confusing for developers
- ✅ Aligned with upstream best practices

The change is backwards compatible and requires no functional modifications - just clearer type annotations.

