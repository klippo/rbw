use crate::{prelude::*, api::KdfType};
use sha2::Digest;
extern crate argon2;
use argon2::{Config, ThreadMode, Variant, Version};

pub struct Identity {
    pub email: String,
    pub keys: crate::locked::Keys,
    pub master_password_hash: crate::locked::PasswordHash,
}

impl Identity {
    pub fn new(
        email: &str,
        password: &crate::locked::Password,
        kdf: KdfType,
        iterations: u32,
        memory: Option<u32>,
        parallelism: Option<u32>,
    ) -> Result<Self> {
        let iterations = std::num::NonZeroU32::new(iterations)
            .ok_or(Error::Pbkdf2ZeroIterations)?;

        let mut keys = crate::locked::Vec::new();
        keys.extend(std::iter::repeat(0).take(64));

        let enc_key = &mut keys.data_mut()[0..32];

        match kdf {
            KdfType::Pbkdf2 => {
                pbkdf2::pbkdf2::<hmac::Hmac<sha2::Sha256>>(
                    password.password(),
                    email.as_bytes(),
                    iterations.get(),
                    enc_key,
                )
                .map_err(|_| Error::Pbkdf2)?;
            }

            KdfType::Argon2id => {
                let mut hasher = sha2::Sha256::new();
                hasher.update(email.as_bytes());
                let salt = hasher.finalize();

                let config = Config {
                    variant: Variant::Argon2id,
                    version: Version::Version13,
                    mem_cost: memory.unwrap() * 1024,
                    time_cost: iterations.get(),
                    lanes: parallelism.unwrap(),
                    thread_mode: ThreadMode::Parallel,
                    secret: &[],
                    ad: &[],
                    hash_length: 32
                };
                let hash = argon2::hash_raw(password.password(), &salt[..], &config).map_err(|_| Error::Argon2)?;
                enc_key.copy_from_slice(&hash);
            }
        };
        
        let mut hash = crate::locked::Vec::new();
        hash.extend(std::iter::repeat(0).take(32));
        pbkdf2::pbkdf2::<hmac::Hmac<sha2::Sha256>>(
            enc_key,
            password.password(),
            1,
            hash.data_mut(),
        )
        .map_err(|_| Error::Pbkdf2)?;

        let hkdf = hkdf::Hkdf::<sha2::Sha256>::from_prk(enc_key)
            .map_err(|_| Error::HkdfExpand)?;
        hkdf.expand(b"enc", enc_key)
            .map_err(|_| Error::HkdfExpand)?;
        let mac_key = &mut keys.data_mut()[32..64];
        hkdf.expand(b"mac", mac_key)
            .map_err(|_| Error::HkdfExpand)?;

        let keys = crate::locked::Keys::new(keys);
        let master_password_hash = crate::locked::PasswordHash::new(hash);

        Ok(Self {
            email: email.to_string(),
            keys,
            master_password_hash,
        })
    }
}
