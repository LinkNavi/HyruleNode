// hyrule-node/src/crypto.rs
use ed25519_dalek::{SigningKey, VerifyingKey, Signature, Signer, Verifier};
use anyhow::Result;

/// Sign data with node's private key
pub fn sign_data(private_key_hex: &str, data: &[u8]) -> Result<Vec<u8>> {
    let private_key_bytes = hex::decode(private_key_hex)?;
    let signing_key = SigningKey::from_bytes(&private_key_bytes.try_into()
        .map_err(|_| anyhow::anyhow!("Invalid private key length"))?);
    
    let signature = signing_key.sign(data);
    Ok(signature.to_bytes().to_vec())
}

/// Verify signature
pub fn verify_signature(public_key_hex: &str, data: &[u8], signature: &[u8]) -> Result<bool> {
    let public_key_bytes = hex::decode(public_key_hex)?;
    let verifying_key = VerifyingKey::from_bytes(&public_key_bytes.try_into()
        .map_err(|_| anyhow::anyhow!("Invalid public key length"))?)?;
    
    let signature = Signature::from_bytes(signature.try_into()
        .map_err(|_| anyhow::anyhow!("Invalid signature length"))?);
    
    Ok(verifying_key.verify(data, &signature).is_ok())
}

/// Hash data using BLAKE3
pub fn hash_data(data: &[u8]) -> String {
    hex::encode(blake3::hash(data).as_bytes())
}

/// Verify object integrity
pub fn verify_object_hash(data: &[u8], expected_hash: &str) -> bool {
    let actual_hash = hash_data(data);
    actual_hash == expected_hash
}
