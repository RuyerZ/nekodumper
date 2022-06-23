use aes::cipher::{block_padding::Pkcs7, BlockDecryptMut, KeyIvInit};
use sha2::{Digest, Sha256};
type Aes256Cbc = cbc::Decryptor<aes::Aes256>;

fn aes_decrypt(mut ciphertext: Vec<u8>, key: &[u8]) -> Option<String> {
    let len = Aes256Cbc::new_from_slices(key, &[0; 16])
        .unwrap() // Unwrap because sha256's length is fixed
        .decrypt_padded_mut::<Pkcs7>(&mut ciphertext)
        .ok()?
        .len();
    ciphertext.truncate(len);
    String::from_utf8(ciphertext).ok()
}

fn sha256(input: &str) -> [u8; 32] {
    let mut hasher = Sha256::new();
    hasher.update(input);
    hasher.finalize().into()
}

pub fn dec<T: AsRef<[u8]>>(content: T, key: &str) -> Option<String> {
    let content: Vec<_> = content
        .as_ref()
        .iter()
        .filter(|x| !x.is_ascii_whitespace())
        .copied()
        .collect();
    let ciphertext = base64::decode(&content).ok()?;
    let key = sha256(key);
    aes_decrypt(ciphertext, &key)
}
