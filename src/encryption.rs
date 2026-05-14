use std::io::{Read, Write};

use age::secrecy::SecretString;

use crate::{Error, Result};

/// Pluggable encryption boundary for at-rest store files.
///
/// Age is the default implementation today; a future PGP backend should
/// implement this trait without changing the store/versioning surface.
pub trait EncryptionMethod: Clone + Send + Sync + 'static {
	fn encrypt(&self, plaintext: &[u8]) -> Result<Vec<u8>>;
	fn decrypt(&self, ciphertext: &[u8]) -> Result<Vec<u8>>;
	fn file_extension(&self) -> &'static str;
}

/// Password-based age encryption using the scrypt recipient.
#[derive(Clone)]
pub struct AgeScrypt {
	passphrase: SecretString,
}

impl AgeScrypt {
	pub fn new(passphrase: impl Into<String>) -> Result<Self> {
		let passphrase = passphrase.into();
		if passphrase.is_empty() {
			return Err(Error::Encryption("passphrase must not be empty".into()));
		}
		Ok(Self { passphrase: SecretString::from(passphrase) })
	}
}

impl EncryptionMethod for AgeScrypt {
	fn encrypt(&self, plaintext: &[u8]) -> Result<Vec<u8>> {
		let encryptor = age::Encryptor::with_user_passphrase(self.passphrase.clone());
		let mut encrypted = Vec::new();
		let mut writer =
			encryptor.wrap_output(&mut encrypted).map_err(|e| Error::Encryption(e.to_string()))?;
		writer.write_all(plaintext)?;
		writer.finish().map_err(|e| Error::Encryption(e.to_string()))?;
		Ok(encrypted)
	}

	fn decrypt(&self, ciphertext: &[u8]) -> Result<Vec<u8>> {
		let decryptor =
			age::Decryptor::new(ciphertext).map_err(|e| Error::Decryption(e.to_string()))?;
		let identity = age::scrypt::Identity::new(self.passphrase.clone());
		let mut reader = decryptor
			.decrypt(std::iter::once(&identity as &dyn age::Identity))
			.map_err(|e| Error::Decryption(e.to_string()))?;
		let mut decrypted = Vec::new();
		reader.read_to_end(&mut decrypted)?;
		Ok(decrypted)
	}

	fn file_extension(&self) -> &'static str { "toml.age" }
}

/// Marker state for a store whose on-disk contents remain encrypted.
#[derive(Debug, Clone, Copy)]
pub struct Locked;

/// Marker state for a store that can decrypt and mutate entries.
#[derive(Debug, Clone)]
pub struct Unlocked<M: EncryptionMethod> {
	pub(crate) method: M,
}

impl<M: EncryptionMethod> Unlocked<M> {
	pub(crate) fn new(method: M) -> Self { Self { method } }
}
