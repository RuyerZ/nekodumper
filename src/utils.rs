use openssl::{
    sha::sha256,
    symm::{decrypt, Cipher},
};

fn aes_decrypt(ciphertext: &[u8], key: &[u8]) -> Option<String> {
    let t = Cipher::aes_256_cbc();
    let v = decrypt(t, key, None, ciphertext).ok()?;
    String::from_utf8(v).ok()
}

pub fn dec(mut content: String, key: &str) -> Option<String> {
    content = content
        .chars()
        .filter(|c| !c.is_ascii_whitespace())
        .collect();
    let ciphertext = base64::decode(&content).ok()?;
    let key = sha256(key.as_bytes());
    aes_decrypt(&ciphertext, &key)
}
