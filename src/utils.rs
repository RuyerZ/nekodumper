use crypto::{aes::{cbc_decryptor,KeySize}, blockmodes::PkcsPadding, buffer::{RefReadBuffer, RefWriteBuffer, WriteBuffer, BufferResult, ReadBuffer}, sha2::Sha256,digest::Digest};

fn aes_decrypt(ciphertext:&[u8],key:&[u8]) -> Option<String> {
    let mut decryptor = cbc_decryptor(KeySize::KeySize256,key,&[0;16],PkcsPadding);
    let mut read_buffer = RefReadBuffer::new(ciphertext);
    let mut buffer = [0; 4096];
    let mut write_buffer = RefWriteBuffer::new(&mut buffer);
    let mut plain = Vec::<u8>::new();
    loop {
        let flag = decryptor.decrypt(&mut read_buffer, &mut write_buffer, true).ok()?;
        plain.extend_from_slice(write_buffer.take_read_buffer().take_remaining());
        if let BufferResult::BufferUnderflow = flag {
            break;
        }
    }
    String::from_utf8(plain).ok()
}

fn sha256(key:&str) -> Vec<u8> {
    let mut engine = Sha256::new();
    engine.input_str(key);
    let mut ret = [0;32];
    engine.result(&mut ret);
    ret.to_vec()
}

pub fn decrypt(mut content:String,key:&str) -> Option<String> {
    content = content.chars().filter(|c| !c.is_ascii_whitespace()).collect();
    let ciphertext = base64::decode(&content).unwrap();
    let key = sha256(key);
    aes_decrypt(&ciphertext, &key)
}