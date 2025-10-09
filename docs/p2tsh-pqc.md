# P2TSH with Post-Quantum Cryptography (SLH-DSA) - Policy Extraction

## Overview

This document describes the policy extraction support for SLH-DSA (SPHINCS+) post-quantum signatures within BDK's P2TSH (Pay-to-TapScript-Hash) descriptors.

## Current Status: Phase 2 Implementation

### Location
`src/descriptor/policy.rs:962-964`

```rust
Terminal::SlhDsaPk(slh_dsa_key) => {
    Some(make_slh_dsa_signature(slh_dsa_key, signers, build_sat))
}
```

### Summary
SLH-DSA public keys now **participate in policy extraction** (Phase 2), making them visible in wallet policy analysis and display mechanisms. However, they are marked as requiring external signing since the current signer infrastructure doesn't fully support post-quantum cryptography.

## Technical Background

### What is Policy Extraction?

Policy extraction is BDK's system for converting low-level miniscript descriptors into human-readable spending conditions. It:

- Translates complex miniscript into understandable spending requirements
- Powers wallet UIs to display what signatures/conditions are needed
- Helps with transaction building and fee estimation
- Provides spending capability analysis

### How Miniscript Terminals Behave

All miniscript terminals now return policy information:

- `Terminal::PkK(pubkey)` → Returns signature policy with signer information
- `Terminal::PkH(pubkey_hash)` → Returns signature policy for hashed keys  
- `Terminal::SlhDsaPk(slh_dsa_key)` → Returns SLH-DSA signature policy (Phase 2: external signer required)
- `Terminal::After(timelock)` → Returns timelock policy
- `Terminal::Multi(threshold)` → Returns multisig threshold policy

## Phase 2 Implementation Details

### What Changed

Phase 2 adds minimal policy support for SLH-DSA keys:

1. **New Policy Type**: Added `SatisfiableItem::SlhDsaSignature` variant
2. **New Key Identifier**: Added `PkOrF::SlhDsaPubkey` variant  
3. **Policy Generation**: `make_slh_dsa_signature()` creates basic policies for SLH-DSA keys
4. **Serialization**: Custom serialization via `SerializableSlhDsaKey` wrapper

### Key Characteristics

- **Visibility**: SLH-DSA keys now appear in policy JSON output
- **Contribution**: Always marked as `Satisfaction::None` (requires external signing)
- **Satisfaction**: Always marked as `Satisfaction::None` (no PSBT support yet)
- **Display**: Serialized as hex string via Display trait

## Current Capabilities & Limitations

### 1. Policy Display ✓ (Phase 2)

**What Works:**
```rust
// examples/p2tsh.rs:276-282
match wallet.policies(KeychainKind::External) {
    Ok(policy) => {
        println!("\n=== Spending Policy ===");
        println!("{}", serde_json::to_string_pretty(&policy)?);
        // ✓ SLH-DSA keys now appear in policy output
    },
    Err(e) => println!("\nNote: Policy extraction: {}", e),
}
```

**Benefits:**
- ✓ TapTrees with SLH-DSA leaves show complete policy structure
- ✓ Pure SLH-DSA descriptors return valid policies
- ✓ Users can see SLH-DSA signature requirements through standard policy APIs
- ✓ Policy JSON includes SLH-DSA keys with type "SLH_DSA_SIGNATURE"

**Example Policy Output:**
```json
{
  "id": "abc123",
  "type": "SLH_DSA_SIGNATURE",
  "slh_dsa_pubkey": "0x1234...abcd",
  "satisfaction": null,
  "contribution": null
}
```

### 2. Remaining Limitations (Phase 2)

Phase 2 provides visibility but not full integration:

| Component | Status | Impact | Notes |
|-----------|--------|--------|-------|
| **Wallet UIs** | ✓ Improved | Now displays SLH-DSA in policies | Marked as "requires external signer" |
| **Fee Estimation** | ⚠️ Limited | Policy shows SLH-DSA but no weight info | Still use `SlhDsaHelper` for accurate fees |
| **Signer Integration** | ❌ Not Available | No built-in SLH-DSA signer support | Must use custom `Satisfier` implementations |
| **PSBT Workflows** | ❌ Not Supported | No standard PSBT fields for SLH-DSA | Proprietary extensions needed |
| **Spending Analysis** | ⚠️ Partial | Can see SLH-DSA requirements | Always shows as "unsatisfied" in policy |
| **Coin Selection** | ⚠️ Manual | Policy doesn't include weight costs | Calculate weights via descriptor methods |

### 3. Design Rationale

**Phase 2 Approach:**

Phase 2 implements minimal policy support without full signer integration:

1. **Visibility Over Integration**: SLH-DSA keys appear in policies but are marked as externally satisfied
2. **Type Safety**: New `SatisfiableItem::SlhDsaSignature` variant provides compile-time correctness
3. **Serialization**: Custom wrapper (`SerializableSlhDsaKey`) handles lack of Serde support
4. **Backward Compatible**: Existing code continues to work; new code can detect SLH-DSA

**Why Not Full Integration?**

Full Phase 3 implementation requires architectural changes:

1. **Signer Infrastructure**: `SignersContainer` needs PQC key support
2. **PSBT Extensions**: No standard fields for 7857-byte SLH-DSA signatures
3. **Weight Estimation**: Policy-based calculation needs PQC-aware weight models
4. **HD Wallet Support**: SLH-DSA key derivation (if applicable) not yet designed

## Current Workarounds

### 1. Custom Satisfier Implementation

The recommended approach is to bypass the policy system entirely:

```rust
// examples/p2tsh.rs:297-307
struct SlhDsaSatisfier {
    slh_dsa_sigs: HashMap<SlhDsaPublicKey, Vec<u8>>,
}

impl<Pk: MiniscriptKey + ToPublicKey> Satisfier<Pk> for SlhDsaSatisfier {
    fn lookup_slh_dsa_sig(&self, pk: &SlhDsaPublicKey) -> Option<Vec<u8>> {
        self.slh_dsa_sigs.get(pk).cloned()
    }
}
```

**Pattern:**
1. Implement custom `Satisfier` trait with `lookup_slh_dsa_sig()`
2. Provide SLH-DSA signatures directly through the satisfier
3. Call `descriptor.satisfy(&satisfier)` to build witness
4. Manually add witness to transaction

### 2. Alternative Fee Estimation

Use `SlhDsaHelper` instead of policy-based estimation:

```rust
// examples/p2tsh.rs:246-248
let fee = SlhDsaHelper::estimate_fee(&descriptor, Amount::from_sat(rate));
println!("Fee @ {} sat/vB: {} sats (~{} BTC)", 
    rate, fee.to_sat(), fee.to_btc());
```

### 3. Manual Weight Calculation

Calculate satisfaction weights directly from the descriptor:

```rust
// examples/p2tsh.rs:222-226
if let Ok(weight) = tsh.max_weight_to_satisfy() {
    println!("\n=== Weight Analysis ===");
    println!("Max satisfaction weight: {} WU", weight.to_wu());
    println!("  (~{} vBytes)", weight.to_vbytes_ceil());
}
```

### 4. Descriptor-Level Checks

Use descriptor extensions to detect SLH-DSA keys:

```rust
// examples/p2tsh.rs:232-239
use bdk_chain::DescriptorExt;
if descriptor.has_slh_dsa_keys() {
    println!("\n✓ Descriptor contains SLH-DSA keys");
    
    if let Some(slh_weight) = descriptor.slh_dsa_witness_weight() {
        println!("  SLH-DSA witness weight: {} WU", slh_weight);
    }
}
```

## Best Practices

### When Using SLH-DSA in P2TSH (Phase 2):

1. **✅ DO:** Use `wallet.policies()` to see SLH-DSA keys in policy structure
2. **✅ DO:** Always use custom satisfiers for spending (policy shows "external signer required")
3. **✅ DO:** Use `SlhDsaHelper` for accurate fee estimation
4. **✅ DO:** Calculate weights manually from descriptors, not from policy
5. **✅ DO:** Check policy output to verify SLH-DSA keys are present
6. **⚠️ DON'T:** Expect `contribution` or `satisfaction` fields to be populated for SLH-DSA
7. **⚠️ DON'T:** Use policy-based weight calculation (doesn't include SLH-DSA signature size)
8. **⚠️ DON'T:** Assume policy alone is sufficient for transaction building

### Recommended User Warning (Phase 2):

```rust
match wallet.policies(KeychainKind::External) {
    Ok(Some(policy)) => {
        println!("Policy: {}", serde_json::to_string_pretty(&policy)?);
        
        if descriptor.has_slh_dsa_keys() {
            println!("\n✓ SLH-DSA keys are visible in policy (Phase 2)");
            println!("⚠️  Important notes:");
            println!("   - SLH-DSA marked as 'external signer required'");
            println!("   - Manual satisfier implementation still required");
            println!("   - Use SlhDsaHelper for accurate fee estimation");
            println!("   - Policy doesn't include ~7857 byte signature weight");
        }
    },
    Ok(None) => println!("No extractable policy"),
    Err(e) => println!("Policy extraction failed: {}", e),
}
```

## Implementation Details

### P2TSH Example Coverage

The `examples/p2tsh.rs` demonstrates proper handling:

- **Lines 175-293**: Full SLH-DSA P2TSH implementation with workarounds
- **Lines 297-318**: Custom satisfier implementation
- **Lines 246-253**: Alternative fee estimation
- **Lines 222-226**: Manual weight calculation
- **Lines 232-239**: Descriptor-level SLH-DSA detection

### Key Dependencies

- `bitcoinpqc`: Provides actual SLH-DSA key generation and signing
- `bdk_chain::slh_dsa_support::SlhDsaHelper`: Fee/weight helpers
- `bdk_chain::DescriptorExt`: Descriptor inspection traits
- Custom `Satisfier` implementation: Transaction building

## Future: Phase 3 (Full Integration)

### Phase 2 → Phase 3 Roadmap:

**Phase 2 (✓ Completed):**
- ✓ `SatisfiableItem::SlhDsaSignature` added
- ✓ `PkOrF::SlhDsaPubkey` added
- ✓ Basic policy extraction working
- ✓ Serialization support via `SerializableSlhDsaKey`

**Phase 3 (Future Work):**

1. **Signer Integration**: 
   - Extend `SignersContainer` to support SLH-DSA keys
   - Add `SignerId` variant for post-quantum keys
   - Implement `lookup_slh_dsa_sig` in standard signers

2. **PSBT Extensions**:
   - Define proprietary PSBT fields for SLH-DSA signatures
   - Support 7857-byte signature storage
   - Add SLH-DSA signing hints

3. **Policy Enhancements**:
   - Populate `contribution` field when SLH-DSA signer is available
   - Include signature size in weight estimation
   - Show "satisfiable" status when keys are in wallet

4. **HD Wallet Support** (if applicable):
   - Define SLH-DSA key derivation scheme
   - Extend `DescriptorPublicKey` for PQC keys with paths

### What Phase 3 Would Enable:

- New policy item types for PQC signatures
- Updated weight/fee estimation in policy layer
- UI/serialization support for PQC spending conditions
- PSBT extensions for PQC signing hints

## Related Files

- `src/descriptor/policy.rs`: Policy extraction implementation
- `examples/p2tsh.rs`: P2TSH with SLH-DSA demonstration
- `examples/policy.rs`: General policy extraction example (traditional keys only)

## Summary

**Phase 2 Status**: SLH-DSA keys are now **visible** in BDK's policy abstraction layer with the following characteristics:

### What Works (Phase 2):
- ✓ SLH-DSA keys appear in policy JSON output
- ✓ Policy type: `SLH_DSA_SIGNATURE`
- ✓ Keys displayed as hex strings
- ✓ Policy structure includes SLH-DSA requirements
- ✓ Backward compatible with existing code

### What Still Requires Workarounds:
- Custom satisfier implementations (policy shows "external signer required")
- Alternative fee/weight estimation via `SlhDsaHelper`
- Manual weight calculation from descriptors
- Direct transaction building (policy doesn't auto-sign)

### For Application Developers:
Phase 2 provides **visibility and structure** for SLH-DSA policies while maintaining the proven patterns in `examples/p2tsh.rs` for actual transaction building. This allows UIs to display post-quantum spending requirements while developers retain full control over the signing process.

---

**Document Status**: Phase 2 Implementation Complete  
**Last Updated**: October 2025  
**Implementation**: Policy extraction with SLH-DSA visibility support  
**Next Steps**: Phase 3 - Full signer integration and PSBT support

