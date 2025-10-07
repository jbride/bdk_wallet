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
//! based on the USE_PQH environment variable.

use bdk_wallet::bitcoin::{Network, secp256k1};
use bdk_wallet::bitcoin::key::{XOnlyPublicKey, Keypair};
use bdk_wallet::fragment;
use bdk_wallet::template::{DescriptorTemplate, P2TSH};
use bdk_wallet::{KeychainKind, Wallet};
use std::str::FromStr;
use std::env;

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
use bitcoin::{ScriptBuf, Address};

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
    let use_pqh = env::var("USE_PQH").unwrap_or_else(|_| "false".to_string()) == "true";
    
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
    
    if use_pqh {
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
    
    // Generate SLH-DSA keypair
    let slh_dsa_keypair = acquire_slh_dsa_keypair();
    println!("Secret key size: {} bytes", secret_key_size(Algorithm::SLH_DSA_128S));
    println!("Public key size: {} bytes", public_key_size(Algorithm::SLH_DSA_128S));
    
    // Create Huffman tree with multiple script leaves
    let huffman_entries = create_huffman_tree_with_slh_dsa(&slh_dsa_keypair);
    println!("✓ Created Huffman tree with {} script leaves", huffman_entries.len());
    
    // Use P2TSH builder with Huffman tree optimization
    let p2tsh_builder = P2tshBuilder::with_huffman_tree(huffman_entries)
        .expect("Failed to create P2TSH builder with Huffman tree");
    
    let p2tsh_spend_info = p2tsh_builder.clone().finalize()
        .expect("Failed to finalize P2TSH spend info");
    
    let merkle_root = p2tsh_spend_info.merkle_root.unwrap();
    
    // Create P2TSH address directly (without BDK wallet)
    let p2tsh_address = Address::p2tsh(Some(merkle_root), network);
    println!("✓ P2TSH Address: {}", p2tsh_address);
    
    Ok(())
}

// Create Huffman tree with SLH-DSA and Schnorr scripts
fn create_huffman_tree_with_slh_dsa(slh_dsa_keypair: &KeyPair) -> Vec<(u32, ScriptBuf)> {
    let mut huffman_entries = vec![];
    
    // Add SLH-DSA script leaf with higher weight (more likely to be used)
    let slh_dsa_script = create_slh_dsa_script(slh_dsa_keypair);
    huffman_entries.push((10, slh_dsa_script));
    
    // Add Schnorr script leaf as fallback with lower weight
    let secp = secp256k1::Secp256k1::new();
    let schnorr_keypair = secp256k1::Keypair::new(&secp, &mut secp256k1::rand::thread_rng());
    let (xonly_pubkey, _parity) = XOnlyPublicKey::from_keypair(&schnorr_keypair);
    let schnorr_script = create_schnorr_script(xonly_pubkey);
    huffman_entries.push((5, schnorr_script));
    
    // Add additional script leaves for demonstration
    for i in 0..3 {
        let additional_keypair = secp256k1::Keypair::new(&secp, &mut secp256k1::rand::thread_rng());
        let (additional_pubkey, _parity) = XOnlyPublicKey::from_keypair(&additional_keypair);
        let additional_script = create_schnorr_script(additional_pubkey);
        huffman_entries.push((2 + i, additional_script));
    }
    
    huffman_entries
}

// Create SLH-DSA script: OP_PUSHBYTES_32 <32-byte pubkey> OP_SUBSTR
fn create_slh_dsa_script(keypair: &KeyPair) -> ScriptBuf {
    let pubkey_bytes = keypair.public_key.bytes.clone();
    let mut script_bytes = vec![0x20]; // OP_PUSHBYTES_32
    script_bytes.extend_from_slice(&pubkey_bytes);
    script_bytes.push(0x7f); // OP_SUBSTR
    ScriptBuf::from_bytes(script_bytes)
}

// Create Schnorr script: OP_PUSHBYTES_32 <32-byte pubkey> OP_CHECKSIG
fn create_schnorr_script(pubkey: XOnlyPublicKey) -> ScriptBuf {
    let mut script_bytes = vec![0x20]; // OP_PUSHBYTES_32
    script_bytes.extend_from_slice(&pubkey.serialize());
    script_bytes.push(0xac); // OP_CHECKSIG
    ScriptBuf::from_bytes(script_bytes)
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
