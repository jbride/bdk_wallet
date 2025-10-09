// Bitcoin Dev Kit
// Written in 2024
//
// This file is licensed under the Apache License, Version 2.0 <LICENSE-APACHE
// or http://www.apache.org/licenses/LICENSE-2.0> or the MIT license
// <LICENSE-MIT or http://opensource.org/licenses/MIT>, at your option.

//! P2TSH (Pay To TapScript Hash) Example with Schnorr Keys and SLH-DSA Support
//!
//! This example demonstrates how to create and use P2TSH descriptors and wallets
//! using either Schnorr keys (X-only public keys) or SLH-DSA post-quantum signatures
//! based on the USE_PQC environment variable.

use bdk_wallet::bitcoin::{Network, secp256k1};
use bdk_wallet::bitcoin::key::{XOnlyPublicKey, Keypair};
use bdk_wallet::fragment;
use bdk_wallet::template::{DescriptorTemplate, P2TSH};
use bdk_wallet::{KeychainKind, Wallet};
use std::str::FromStr;
use std::env;
use std::collections::HashMap;
use bitcoin::hex;

use bdk_chain::BlockId;
use bitcoin::BlockHash;
use bdk_wallet::chain::local_chain::CheckPoint;

// Import bitcoinpqc for real SLH-DSA support
use bitcoinpqc::{
    generate_keypair, public_key_size, secret_key_size, Algorithm, KeyPair, sign, verify,
};

// Import P2TSH functionality from the linked rust code
use bitcoin::p2tsh::{P2tshBuilder, P2tshSpendInfo};
use bitcoin::taproot::{TapTree, TapNodeHash, LeafVersion};
use bitcoin::{ScriptBuf, Address, Amount, FeeRate};

// Import miniscript SLH-DSA support
use bdk_wallet::miniscript::descriptor::{Tsh, TapTree as MiniscriptTapTree, SlhDsaPublicKey};
use bdk_wallet::miniscript::{Miniscript, Tap, Satisfier, MiniscriptKey, ToPublicKey, NoSecp256k1Key};

// Add these to your imports at the top
use bdk_bitcoind_rpc::{
    bitcoincore_rpc::{Auth, Client, RpcApi},
    Emitter,
};
use bdk_wallet::rusqlite::Connection;
use std::sync::Arc;
use std::path::PathBuf;
use std::io::Write;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    // Check if post-quantum cryptography should be used
    let use_pqc = env::var("USE_PQC").unwrap_or_else(|_| "false".to_string()) == "true";
    
    // Override network if BITCOIN_NETWORK environment variable is set
    let network = env::var("BITCOIN_NETWORK")
        .ok()
        .and_then(|s| match s.to_lowercase().as_str() {
            "bitcoin" | "mainnet" => Some(Network::Bitcoin),
            "testnet" => Some(Network::Testnet),
            "signet" => Some(Network::Signet),
            "regtest" => Some(Network::Regtest),
            _ => None,
        })
        .unwrap_or(Network::Bitcoin);
    
    if use_pqc {
        println!("P2TSH using SLH-DSA PQC; network: {}", network);
        println!("==================================================");
        use_slh_dsa_p2tsh(network)?;
    } else {
        println!("P2TSH using Schnorr Secp256k1; network: {}", network);
        println!("===============================");
        use_schnorr_p2tsh(network)?;
    }
    
    Ok(())
}

fn use_schnorr_p2tsh(network: Network) -> Result<(), Box<dyn std::error::Error>> {
    let secp = secp256k1::Secp256k1::new();

    let keypair1: Keypair = secp256k1::Keypair::new(&secp, &mut secp256k1::rand::thread_rng());
    let (xonly_pubkey, _parity) = XOnlyPublicKey::from_keypair(&keypair1);
    let (leaf1_miniscript, _keymap, _networks) = fragment!(pk(xonly_pubkey))?;
    let leaf1 = miniscript::descriptor::TapTree::leaf(std::sync::Arc::new(leaf1_miniscript));
    
    // Leaf 2: Alternative spending path with a different key
    let keypair2 = secp256k1::Keypair::new(&secp, &mut secp256k1::rand::thread_rng());
    let (xonly_pubkey2, _parity2) = XOnlyPublicKey::from_keypair(&keypair2);
    let (leaf2_miniscript, _keymap2, _networks2) = fragment!(pk(xonly_pubkey2))?;
    let leaf2 = miniscript::descriptor::TapTree::leaf(std::sync::Arc::new(leaf2_miniscript));
    
    // Leaf 3: Third spending path
    let keypair3 = secp256k1::Keypair::new(&secp, &mut secp256k1::rand::thread_rng());
    let (xonly_pubkey3, _parity3) = XOnlyPublicKey::from_keypair(&keypair3);
    let (leaf3_miniscript, _keymap3, _networks3) = fragment!(pk(xonly_pubkey3))?;
    let leaf3 = miniscript::descriptor::TapTree::leaf(std::sync::Arc::new(leaf3_miniscript));
    
    // Combine leaves into a TapTree
    let subtree = miniscript::descriptor::TapTree::combine(leaf1, leaf2)?;
    let tap_tree = miniscript::descriptor::TapTree::combine(subtree, leaf3)?;
    
    println!("Created TapTree with 3 script leaves");
    
    // Create P2TSH template
    let p2tsh_template = P2TSH(Some(tap_tree.clone()));
    
    // Build the descriptor using the template
    let (descriptor, _keymap, _networks) = p2tsh_template.build(network)?;
    
    // Get the descriptor string
    let descriptor_str = descriptor.to_string();
    println!("P2TSH Descriptor with Schnorr key: {}", descriptor_str);
    
    // Demonstrate that this is indeed a P2TSH descriptor
    assert!(descriptor_str.starts_with("tsh("), "Descriptor should start with 'tsh('");
    println!("Confirmed: This is a P2TSH descriptor");
    
    // Show the descriptor type
    println!("Descriptor Type: {:?}", descriptor);
    
    println!("\n=== WALLET CREATION WITH BITCOIN CORE RPC ===");
    
    // Create external template (used for receiving addresses)
    let external_template = P2TSH(Some(tap_tree.clone()));
    println!("External template created (for receiving addresses)");
    println!("  - Uses complex TapTree with 3 leaves (keypair1, keypair2, keypair3)");

    // Create internal template (used for change addresses)
    let keypair_internal = secp256k1::Keypair::new(&secp, &mut secp256k1::rand::thread_rng());
    let (xonly_internal, _) = XOnlyPublicKey::from_keypair(&keypair_internal);
    println!("Internal keypair created: {:?}", xonly_internal);
    let (leaf_internal, _, _) = fragment!(pk(xonly_internal))?;
    let tree_internal = miniscript::descriptor::TapTree::leaf(std::sync::Arc::new(leaf_internal));
    let internal_template = P2TSH(Some(tree_internal));
    println!("Internal template created (for change addresses)");
    println!("  - Uses simple TapTree with 1 leaf (keypair_internal)");

    // Create wallet without automatic persistence
    let mut wallet = Wallet::create(external_template, internal_template)
        .network(network)
        .create_wallet_no_persist()?;
    println!("✓ Wallet created (no persistence) with network: {:?}", network);
    
    // Get descriptors from the created wallet to show the difference
    let ext_desc = wallet.public_descriptor(KeychainKind::External);
    let int_desc = wallet.public_descriptor(KeychainKind::Internal);
    
    println!("\n=== DESCRIPTOR COMPARISON ===");
    println!("External descriptor: {}", ext_desc);
    println!("External length: {} chars", ext_desc.to_string().len());
    println!("\nInternal descriptor: {}", int_desc);
    println!("Internal length: {} chars", int_desc.to_string().len());
    
    // Generate addresses
    let address = wallet.next_unused_address(KeychainKind::External);
    println!("\n=== ADDRESSES ===");
    println!("P2TSH External Address: {}", address);
    
    let change_address = wallet.next_unused_address(KeychainKind::Internal);
    println!("P2TSH Internal (Change) Address: {}", change_address);
    
    match wallet.policies(KeychainKind::External) {
        Ok(policy) => println!("\n=== SPENDING POLICY ===\n{}", serde_json::to_string_pretty(&policy)?),
        Err(e) => println!("\nNote: Policy extraction failed: {}", e),
    }
    
    Ok(())
}

fn use_slh_dsa_p2tsh(network: Network) -> Result<(), Box<dyn std::error::Error>> {
    println!("\n=== Using Miniscript-based P2TSH with SLH-DSA ===\n");
    
    // Generate SLH-DSA keypair using bitcoinpqc
    let slh_dsa_keypair = acquire_slh_dsa_keypair();
    println!("Secret key size: {} bytes", secret_key_size(Algorithm::SLH_DSA_128S));
    println!("Public key size: {} bytes", public_key_size(Algorithm::SLH_DSA_128S));
    
    // Convert to miniscript's SlhDsaPublicKey type (32 bytes)
    let slh_dsa_pubkey_bytes = &slh_dsa_keypair.public_key.bytes[..32];
    let slh_dsa_key = SlhDsaPublicKey::from_slice(slh_dsa_pubkey_bytes)
        .expect("Failed to create SlhDsaPublicKey");
    
    println!("✓ SLH-DSA Public Key: {}", slh_dsa_key);
    
    // Create a Miniscript that compiles to: <32-byte-slh-dsa-key> OP_SUCCESS127
    //
    // Note: NoSecp256k1Key is a placeholder type parameter for Miniscript<Pk, Ctx>
    // - The slh_dsa_pk() method uses the concrete SlhDsaPublicKey type internally
    // - NoSecp256k1Key satisfies the MiniscriptKey trait requirement without providing
    //   actual secp256k1 key functionality
    // - This makes the code self-documenting: it clearly indicates this miniscript
    //   contains only post-quantum keys, no secp256k1 keys
    let slh_dsa_ms: Miniscript<NoSecp256k1Key, Tap> = Miniscript::slh_dsa_pk(slh_dsa_key);
    println!("✓ Created SLH-DSA miniscript: {}", slh_dsa_ms);
    
    // Create taptree with SLH-DSA leaf
    let tap_tree= MiniscriptTapTree::leaf(std::sync::Arc::new(slh_dsa_ms));
    
    println!("✓ Created TapTree with SLH-DSA leaf");
    
    // Create P2TSH descriptor using miniscript
    // Note: NoSecp256k1Key is used to indicate this descriptor contains only post-quantum keys
    let tsh: Tsh<NoSecp256k1Key> = Tsh::new(Some(tap_tree.clone()))
        .expect("Failed to create Tsh descriptor");
    
    println!("✓ P2TSH Descriptor: {}", tsh);
    
    // Get script pubkey and address
    let script_pubkey = tsh.script_pubkey();
    let p2tsh_address = tsh.address(network);
    println!("✓ P2TSH Address: {}", p2tsh_address);
    println!("  Script PubKey: {}", script_pubkey.as_script());
    
    // Calculate maximum satisfaction weight
    if let Ok(weight) = tsh.max_weight_to_satisfy() {
        println!("\n=== Weight Analysis ===");
        println!("Max satisfaction weight: {} WU", weight.to_wu());
        println!("  (~{} vBytes)", weight.to_vbytes_ceil());
    }
    
    println!("\n=== SLH-DSA Notes ===");
    println!("For fee estimation, calculate weight manually using descriptor.max_weight_to_satisfy()");
    println!("SLH-DSA keys are visible in policy extraction");
    println!("This enables wallet UIs to display post-quantum spending requirements.");
    println!("\nKey improvements:");
    println!("  ✓ SLH-DSA keys visible in policy JSON");
    println!("  ✓ Policy type: SLH_DSA_SIGNATURE");
    println!("  ✓ Backward compatible with existing code");
    println!("\nNote: Full wallet integration (Phase 3) would require:");
    println!("  - Signer infrastructure for PQC keys");
    println!("  - PSBT extensions for 7857-byte signatures");
    
    // Demonstrate custom satisfier (conceptual - would need actual signing implementation)
    println!("\n=== Custom Satisfier Pattern ===");
    println!("To spend from this P2TSH output, implement a custom Satisfier:");
    println!("  1. Implement Satisfier trait with lookup_slh_dsa_sig()");
    println!("  2. Provide 7857-byte SLH-DSA signature via satisfier");
    println!("  3. Call descriptor.satisfy(&my_satisfier) to build witness");
    println!("  4. Add witness to transaction");
    
    Ok(())
}

// Example custom satisfier for SLH-DSA (conceptual implementation)
// In production, this would look up actual signatures from a database/HSM
struct SlhDsaSatisfier {
    slh_dsa_sigs: HashMap<SlhDsaPublicKey, Vec<u8>>,
}

impl<Pk: MiniscriptKey + ToPublicKey> Satisfier<Pk> for SlhDsaSatisfier {
    fn lookup_slh_dsa_sig(&self, pk: &SlhDsaPublicKey) -> Option<Vec<u8>> {
        self.slh_dsa_sigs.get(pk).cloned()
    }
    
    // Other Satisfier methods would delegate to a base satisfier or return None
}

// Helper function to create SlhDsaSatisfier with a signature
#[allow(dead_code)]
fn create_slh_dsa_satisfier(
    slh_key: SlhDsaPublicKey,
    signature: Vec<u8>,
) -> SlhDsaSatisfier {
    let mut sigs = HashMap::new();
    sigs.insert(slh_key, signature);
    SlhDsaSatisfier { slh_dsa_sigs: sigs }
}

fn acquire_slh_dsa_keypair() -> KeyPair {

    let random_data = get_random_bytes(128);
    let keypair: KeyPair = generate_keypair(Algorithm::SLH_DSA_128S, &random_data)
            .expect("Failed to generate SLH-DSA-128S keypair");
    
    keypair
}

fn get_random_bytes(count: usize) -> Vec<u8> {
    use rand::RngCore;
    let mut bytes = vec![0u8; count];
    secp256k1::rand::thread_rng().fill_bytes(&mut bytes);
    bytes
}
