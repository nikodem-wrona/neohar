//! The encryption of a push (RFC 8291, `aes128gcm` of RFC 8188). The push service sees only the
//! encrypted bytes.
//!
//! The sender makes a new P-256 key and a new salt for each push. It agrees a secret with the key
//! of the browser (ECDH), mixes in the authentication secret of the browser (HKDF), and encrypts
//! the message as one record with AES-128-GCM. The body is the header (salt, record size, the
//! public key of the sender) and the record.

use aes_gcm::{
    Aes128Gcm, KeyInit,
    aead::{Aead, Payload},
};
use hkdf::Hkdf;
use p256::{PublicKey, SecretKey, elliptic_curve::Generate};
use sha2::Sha256;

/// The record size in the header. A push has one record.
const RECORD_SIZE: u32 = 4096;

/// The size of the header: salt (16), record size (4), key length (1) and the key (65).
const HEADER: usize = 16 + 4 + 1 + 65;

/// The tag of AES-GCM.
const TAG: usize = 16;

/// The longest message: the body of a push has at most 4096 bytes.
pub const MAX_MESSAGE: usize = 4096 - HEADER - TAG - 1;

/// The keys of a subscription, decoded.
pub struct Receiver {
    /// The public key of the browser (`p256dh`).
    pub public: PublicKey,
    /// The authentication secret of the browser (`auth`), 16 bytes.
    pub auth: [u8; 16],
}

impl Receiver {
    /// The keys from the bytes of `p256dh` (65, uncompressed) and `auth` (16).
    pub fn new(p256dh: &[u8], auth: &[u8]) -> Result<Self, String> {
        if p256dh.len() != 65 || p256dh.first() != Some(&4) {
            return Err("p256dh is not an uncompressed P-256 key of 65 bytes".to_owned());
        }
        let public = PublicKey::from_sec1_bytes(p256dh).map_err(|_| "p256dh is not a P-256 key")?;
        let auth: [u8; 16] = auth
            .try_into()
            .map_err(|_| "auth is not a secret of 16 bytes")?;
        Ok(Self { public, auth })
    }
}

/// Encrypts a message for a subscription, with a new key and a new salt.
pub fn encrypt(receiver: &Receiver, message: &[u8]) -> Result<Vec<u8>, String> {
    let sender = SecretKey::generate();
    let mut salt = [0u8; 16];
    getrandom::fill(&mut salt).map_err(|error| error.to_string())?;
    encrypt_with(&sender, &salt, receiver, message)
}

/// Encrypts with this key and this salt. The test of RFC 8291 gives both.
fn encrypt_with(
    sender: &SecretKey,
    salt: &[u8; 16],
    receiver: &Receiver,
    message: &[u8],
) -> Result<Vec<u8>, String> {
    if message.len() > MAX_MESSAGE {
        return Err(format!(
            "the message has {} bytes, more than {MAX_MESSAGE}",
            message.len()
        ));
    }
    let sender_public = sec1(&sender.public_key());
    let receiver_public = sec1(&receiver.public);
    let shared = sender.diffie_hellman(&receiver.public);
    let (key, nonce) = keys(
        shared.raw_secret_bytes(),
        &receiver.auth,
        &receiver_public,
        &sender_public,
        salt,
    )?;
    let mut record = Vec::with_capacity(message.len() + 1);
    record.extend_from_slice(message);
    // The delimiter of the last record, with no padding.
    record.push(2);
    let cipher = Aes128Gcm::new_from_slice(&key).map_err(|error| error.to_string())?;
    let sealed = cipher
        .encrypt(
            (&nonce).into(),
            Payload {
                msg: &record,
                aad: &[],
            },
        )
        .map_err(|error| error.to_string())?;
    let mut body = Vec::with_capacity(HEADER + sealed.len());
    body.extend_from_slice(salt);
    body.extend_from_slice(&RECORD_SIZE.to_be_bytes());
    body.push(65);
    body.extend_from_slice(&sender_public);
    body.extend_from_slice(&sealed);
    Ok(body)
}

/// Decrypts a body with the secret key of the browser: what the push service of the tests does.
pub fn decrypt(receiver: &SecretKey, auth: &[u8; 16], body: &[u8]) -> Result<Vec<u8>, String> {
    if body.len() < HEADER + TAG + 1 {
        return Err("the body is shorter than a header and a record".to_owned());
    }
    let (header, sealed) = body.split_at(HEADER);
    let salt: [u8; 16] = header[..16].try_into().map_err(|_| "no salt")?;
    if header[20] != 65 {
        return Err("the key id is not a P-256 key".to_owned());
    }
    let sender_public = &header[21..HEADER];
    let sender = PublicKey::from_sec1_bytes(sender_public).map_err(|_| "a bad sender key")?;
    let receiver_public = sec1(&receiver.public_key());
    let shared = receiver.diffie_hellman(&sender);
    let (key, nonce) = keys(
        shared.raw_secret_bytes(),
        auth,
        &receiver_public,
        sender_public,
        &salt,
    )?;
    let cipher = Aes128Gcm::new_from_slice(&key).map_err(|error| error.to_string())?;
    let mut record = cipher
        .decrypt(
            (&nonce).into(),
            Payload {
                msg: sealed,
                aad: &[],
            },
        )
        .map_err(|_| "the record does not decrypt")?;
    match record.pop() {
        Some(2) => Ok(record),
        _ => Err("the record has no delimiter of a last record".to_owned()),
    }
}

/// The content encryption key and the nonce (RFC 8291 §3.4).
fn keys(
    shared: &[u8],
    auth: &[u8; 16],
    receiver_public: &[u8],
    sender_public: &[u8],
    salt: &[u8; 16],
) -> Result<([u8; 16], [u8; 12]), String> {
    let mut info = Vec::with_capacity(14 + 65 + 65);
    info.extend_from_slice(b"WebPush: info\0");
    info.extend_from_slice(receiver_public);
    info.extend_from_slice(sender_public);
    let mut ikm = [0u8; 32];
    Hkdf::<Sha256>::new(Some(auth), shared)
        .expand(&info, &mut ikm)
        .map_err(|error| error.to_string())?;
    let content = Hkdf::<Sha256>::new(Some(salt), &ikm);
    let mut key = [0u8; 16];
    content
        .expand(b"Content-Encoding: aes128gcm\0", &mut key)
        .map_err(|error| error.to_string())?;
    let mut nonce = [0u8; 12];
    content
        .expand(b"Content-Encoding: nonce\0", &mut nonce)
        .map_err(|error| error.to_string())?;
    Ok((key, nonce))
}

/// The uncompressed point of a key: 65 bytes.
pub fn sec1(key: &PublicKey) -> Vec<u8> {
    key.to_sec1_bytes().into_vec()
}

#[cfg(test)]
mod tests {
    use base64::{Engine, engine::general_purpose::URL_SAFE_NO_PAD};

    use super::*;

    fn b64(text: &str) -> Vec<u8> {
        let text: String = text.split_whitespace().collect();
        URL_SAFE_NO_PAD.decode(text).expect("base64url")
    }

    /// The example of RFC 8291, section 5 and appendix A.
    #[test]
    fn the_example_of_rfc_8291_encrypts_to_its_body() {
        let sender = SecretKey::from_slice(&b64("yfWPiYE-n46HLnH0KqZOF1fJJU3MYrct3AELtAQ-oRw"))
            .expect("as_private");
        assert_eq!(
            sec1(&sender.public_key()),
            b64("BP4z9KsN6nGRTbVYI_c7VJSPQTBtkgcy27mlmlMoZIIg
                 Dll6e3vCYLocInmYWAmS6TlzAC8wEqKK6PBru3jl7A8"),
            "as_public"
        );
        let receiver = Receiver::new(
            &b64("BCVxsr7N_eNgVRqvHtD0zTZsEc6-VV-JvLexhqUzORcx
                  aOzi6-AYWXvTBHm4bjyPjs7Vd8pZGH6SRpkNtoIAiw4"),
            &b64("BTBZMqHH6r4Tts7J_aSIgg"),
        )
        .expect("the keys of the browser");
        let salt: [u8; 16] = b64("DGv6ra1nlYgDCS1FRnbzlw").try_into().expect("16 bytes");
        let message = b64("V2hlbiBJIGdyb3cgdXAsIEkgd2FudCB0byBiZSBhIHdhdGVybWVsb24");
        assert_eq!(message, b"When I grow up, I want to be a watermelon");

        let body = encrypt_with(&sender, &salt, &receiver, &message).expect("a body");
        let header = b64(
            "DGv6ra1nlYgDCS1FRnbzlwAAEABBBP4z 9KsN6nGRTbVYI_c7VJSPQTBtkgcy27ml
                          mlMoZIIgDll6e3vCYLocInmYWAmS6Tlz AC8wEqKK6PBru3jl7A8",
        );
        let sealed = b64(
            "8pfeW0KbunFT06SuDKoJH9Ql87S1QUrd irN6GcG7sFz1y1sqLgVi1VhjVkHsUoEs
                          bI_0LpXMuGvnzQ",
        );
        assert_eq!(header.len(), 86);
        assert_eq!(&body[..86], header.as_slice(), "the header");
        assert_eq!(&body[86..], sealed.as_slice(), "the record");

        let ua_private = SecretKey::from_slice(&b64("q1dXpw3UpT5VOmu_cf_v6ih07Aems3njxI-JWgLcM94"))
            .expect("ua_private");
        assert_eq!(
            decrypt(&ua_private, &receiver.auth, &body).expect("decrypts"),
            message
        );
    }

    #[test]
    fn a_message_goes_there_and_back_with_a_new_key_each_time() {
        let browser = SecretKey::generate();
        let receiver = Receiver {
            public: browser.public_key(),
            auth: [7; 16],
        };
        let first = encrypt(&receiver, b"{\"title\":\"neohar\"}").expect("a body");
        let second = encrypt(&receiver, b"{\"title\":\"neohar\"}").expect("a body");
        assert_ne!(first, second, "a new salt and a new key");
        assert_eq!(
            decrypt(&browser, &[7; 16], &first).expect("decrypts"),
            b"{\"title\":\"neohar\"}"
        );
        assert!(
            decrypt(&browser, &[8; 16], &first).is_err(),
            "another secret"
        );
        assert!(encrypt(&receiver, &vec![b'x'; MAX_MESSAGE]).is_ok());
        assert!(encrypt(&receiver, &vec![b'x'; MAX_MESSAGE + 1]).is_err());
    }

    #[test]
    fn bad_keys_of_a_subscription_fail() {
        let good = sec1(&SecretKey::generate().public_key());
        assert!(Receiver::new(&good, &[0; 16]).is_ok());
        assert!(Receiver::new(&good[..64], &[0; 16]).is_err(), "too short");
        assert!(Receiver::new(&good, &[0; 15]).is_err(), "a short secret");
        let mut compressed = good.clone();
        compressed[0] = 2;
        assert!(Receiver::new(&compressed, &[0; 16]).is_err());
        let mut off_curve = good;
        off_curve[64] ^= 1;
        assert!(
            Receiver::new(&off_curve, &[0; 16]).is_err(),
            "not on the curve"
        );
    }
}
