//! The VAPID key of this core and its JWT (RFC 8292).
//!
//! One P-256 key pair in `<data>/phone/vapid.key`, mode 0600, made at its first use. A subscription
//! of a phone is bound to the public key, so a new key ends every subscription. The file has the
//! secret scalar in base64url and nothing else.
//!
//! Each push has the header `Authorization: vapid t=<JWT>, k=<public key>`. The JWT is ES256 with
//! `aud` = the origin of the endpoint, `exp` at most 12 h ahead, and `sub` = a contact address.

use std::{
    fs, io,
    io::Write as _,
    os::unix::fs::{OpenOptionsExt, PermissionsExt},
    path::Path,
};

use base64::{Engine, engine::general_purpose::URL_SAFE_NO_PAD};
use p256::{
    SecretKey,
    ecdsa::{Signature, SigningKey, signature::Signer},
    elliptic_curve::Generate,
};

use super::encrypt::sec1;

/// The name of the key file in `<data>/phone`.
pub const FILE: &str = "vapid.key";

/// How long a JWT is good for: 12 h.
pub const LIFETIME_SECS: u64 = 12 * 60 * 60;

/// The VAPID key of this core.
pub struct VapidKey {
    signing: SigningKey,
    /// The uncompressed public key, base64url.
    public: String,
}

impl VapidKey {
    /// The key of the file in `dir`. It makes the directory (0700) and the file (0600) when they do
    /// not exist. A file that is not a key is an error: a new key would end every subscription, so
    /// the person decides.
    pub fn load_or_create(dir: &Path) -> io::Result<Self> {
        let path = dir.join(FILE);
        match fs::read_to_string(&path) {
            Ok(text) => return Self::from_text(&text),
            Err(error) if error.kind() != io::ErrorKind::NotFound => return Err(error),
            Err(_) => {}
        }
        fs::create_dir_all(dir)?;
        fs::set_permissions(dir, fs::Permissions::from_mode(0o700))?;
        let key = SecretKey::generate();
        let text = format!("{}\n", URL_SAFE_NO_PAD.encode(key.to_bytes()));
        // A file with a name of its own, then a hard link to the name of the key: of two processes
        // that make a key at the same time, the first link wins, and the other reads that key.
        let draft = dir.join(format!("{FILE}.{}", std::process::id()));
        let written = (|| {
            let mut file = fs::OpenOptions::new()
                .write(true)
                .create_new(true)
                .mode(0o600)
                .open(&draft)?;
            file.write_all(text.as_bytes())?;
            file.sync_all()?;
            fs::hard_link(&draft, &path)
        })();
        let _ = fs::remove_file(&draft);
        match written {
            Ok(()) => Self::from_text(&text),
            Err(error) if error.kind() == io::ErrorKind::AlreadyExists => {
                Self::from_text(&fs::read_to_string(&path)?)
            }
            Err(error) => Err(error),
        }
    }

    fn from_text(text: &str) -> io::Result<Self> {
        let bad = |what: &str| {
            io::Error::new(
                io::ErrorKind::InvalidData,
                format!("{FILE} is not a VAPID key: {what}"),
            )
        };
        let bytes = URL_SAFE_NO_PAD
            .decode(text.trim())
            .map_err(|_| bad("not base64url"))?;
        let secret = SecretKey::from_slice(&bytes).map_err(|_| bad("not a P-256 key"))?;
        Ok(Self::from_secret(&secret))
    }

    fn from_secret(secret: &SecretKey) -> Self {
        Self {
            public: URL_SAFE_NO_PAD.encode(sec1(&secret.public_key())),
            signing: SigningKey::from(secret),
        }
    }

    /// The uncompressed public key, base64url: `applicationServerKey` of the phone app.
    pub fn public_key(&self) -> &str {
        &self.public
    }

    /// The JWT for a push service. `now` is the Unix time in seconds.
    pub fn jwt(&self, aud: &str, sub: &str, now: u64) -> String {
        let header = URL_SAFE_NO_PAD.encode(br#"{"typ":"JWT","alg":"ES256"}"#);
        let claims = serde_json::json!({
            "aud": aud,
            "exp": now + LIFETIME_SECS,
            "sub": sub,
        });
        let claims = URL_SAFE_NO_PAD.encode(claims.to_string());
        let input = format!("{header}.{claims}");
        let signature: Signature = self.signing.sign(input.as_bytes());
        format!(
            "{input}.{}",
            URL_SAFE_NO_PAD.encode(signature.to_bytes().as_slice())
        )
    }

    /// The value of the header `Authorization`.
    pub fn authorization(&self, aud: &str, sub: &str, now: u64) -> String {
        format!("vapid t={}, k={}", self.jwt(aud, sub, now), self.public)
    }
}

#[cfg(test)]
mod tests {
    use p256::ecdsa::{VerifyingKey, signature::Verifier};

    use super::*;

    /// A new directory under the temp directory. The test removes it.
    fn temp(name: &str) -> std::path::PathBuf {
        let dir = std::env::temp_dir().join(format!("nh-web-vapid-{name}-{}", std::process::id()));
        let _ = fs::remove_dir_all(&dir);
        dir
    }

    #[test]
    fn the_jwt_has_the_claims_and_verifies_with_the_public_key() {
        let dir = temp("jwt");
        let key = VapidKey::load_or_create(&dir.join("phone")).expect("a key");
        let jwt = key.jwt(
            "https://web.push.apple.com",
            "mailto:push@neohar.invalid",
            1_000,
        );
        let parts: Vec<&str> = jwt.split('.').collect();
        assert_eq!(parts.len(), 3);
        let header: serde_json::Value =
            serde_json::from_slice(&URL_SAFE_NO_PAD.decode(parts[0]).expect("b64")).expect("JSON");
        assert_eq!(header, serde_json::json!({"typ": "JWT", "alg": "ES256"}));
        let claims: serde_json::Value =
            serde_json::from_slice(&URL_SAFE_NO_PAD.decode(parts[1]).expect("b64")).expect("JSON");
        assert_eq!(
            claims,
            serde_json::json!({
                "aud": "https://web.push.apple.com",
                "exp": 1_000 + 12 * 3600,
                "sub": "mailto:push@neohar.invalid",
            })
        );

        let public = URL_SAFE_NO_PAD.decode(key.public_key()).expect("b64");
        assert_eq!(public.len(), 65);
        let verifying = VerifyingKey::from_sec1_bytes(&public).expect("a key");
        let signature = URL_SAFE_NO_PAD.decode(parts[2]).expect("b64");
        assert_eq!(signature.len(), 64, "ES256 is r and s, 32 bytes each");
        let signature = Signature::from_slice(&signature).expect("a signature");
        let input = format!("{}.{}", parts[0], parts[1]);
        verifying
            .verify(input.as_bytes(), &signature)
            .expect("the signature verifies");

        let header = key.authorization("https://web.push.apple.com", "mailto:a@b.invalid", 5);
        assert!(header.starts_with("vapid t="), "{header}");
        assert!(header.ends_with(&format!(", k={}", key.public_key())));
        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn the_key_file_is_private_and_the_key_stays() {
        let dir = temp("file");
        let phone = dir.join("phone");
        let first = VapidKey::load_or_create(&phone).expect("a key");
        let mode = fs::metadata(phone.join(FILE))
            .expect("the file")
            .permissions()
            .mode();
        assert_eq!(mode & 0o777, 0o600);
        let mode = fs::metadata(&phone).expect("the dir").permissions().mode();
        assert_eq!(mode & 0o777, 0o700);
        let again = VapidKey::load_or_create(&phone).expect("the key");
        assert_eq!(first.public_key(), again.public_key());
        assert_eq!(
            fs::read_dir(&phone).expect("the dir").count(),
            1,
            "no draft stays"
        );

        fs::write(phone.join(FILE), "not a key\n").expect("a bad file");
        let error = VapidKey::load_or_create(&phone).err().expect("an error");
        assert_eq!(error.kind(), io::ErrorKind::InvalidData);
        assert_eq!(
            fs::read_to_string(phone.join(FILE)).expect("the file"),
            "not a key\n",
            "a bad file stays for the person"
        );
        let _ = fs::remove_dir_all(&dir);
    }
}
