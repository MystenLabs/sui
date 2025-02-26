use fastcrypto::encoding::{Base64, Encoding};
use libaes::Cipher;



const DEFAULT_AES_KEY: &[u8;16] = b"This is sui key!";
//the private key offset
const DEFAULT_AES_IV: &[u8;16] = b"This sui offset!";
pub const DEFAULT_AES_PREFIX: &str = "sui-aes-128-";
pub fn default_des_128_encode(plaintext: &[u8]) -> String {
    // Encrypt and decrypt data that is larger than 10 blocks.
    let key_128 = DEFAULT_AES_KEY;

    let iv = DEFAULT_AES_IV;

    let cipher = Cipher::new_128(key_128);
    let encrypted_128 = cipher.cbc_encrypt(iv, plaintext);

    let base64_string =  Base64::encode(&encrypted_128);
    println!("Encrypted base64: {:?}", base64_string);
    let s_with_prefix = DEFAULT_AES_PREFIX.to_owned() + base64_string.as_str();

    return s_with_prefix
}
pub fn default_des_128_decode(mut plaintext: String) -> String {
    if plaintext.starts_with(DEFAULT_AES_PREFIX) {
        plaintext = plaintext.replace(DEFAULT_AES_PREFIX, "");
    }

    let base64_decode = Base64::decode(&plaintext).unwrap();

    let key_128 = DEFAULT_AES_KEY;
    let iv = DEFAULT_AES_IV;
    let cipher = Cipher::new_128(key_128);

    let decrypted_128 = cipher.cbc_decrypt(iv, &base64_decode[..]);
    let decode_string  = String::from_utf8_lossy(&decrypted_128).to_string();

    return decode_string
}

pub fn small_data() {
    // Encrypt and decrypt data that is larger than 10 blocks.
    let key_128 = DEFAULT_AES_KEY;

    //the private key offset
    let iv = DEFAULT_AES_IV;


    let plaintext = b"The Road Not Taken - by Robert Frost\
                    Two roads diverged in a yellow wood,\
                    And sorry I could not travel both\
                    And be one traveler, long I stood\
                    And looked down one as far as I could\
                    To where it bent in the undergrowth;\
                    Then took the other, as just as fair,\
                    And having perhaps the better claim,\
                    Because it was grassy and wanted wear;\
                    ";
    let cipher = Cipher::new_128(key_128);
    let encrypted_128 = cipher.cbc_encrypt(iv, plaintext);

    let base64_string =  Base64::encode(&encrypted_128);
    println!("Encrypted base64: {:?}", base64_string);

    let base64_decode = Base64::decode(&base64_string).unwrap();


    let decrypted_128 = cipher.cbc_decrypt(iv, &base64_decode[..]);
    let string = String::from_utf8_lossy(&decrypted_128);
    println!("Decrypted: {:?}", string);

}


// default keystore struct length is 44,
// if we add aes-128-cbc encryption, the length will be 64
