use std::fs;
use std::path::Path;

/// API keys. The Windows app encrypts these with DPAPI before writing them.
#[derive(Clone, PartialEq, Eq, Default)]
pub struct Secrets {
    pub deepl_key: String,
    pub azure_key: String,
}

impl std::fmt::Debug for Secrets {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Secrets")
            .field("deepl_key", &redact(&self.deepl_key))
            .field("azure_key", &redact(&self.azure_key))
            .finish()
    }
}

fn redact(key: &str) -> String {
    if key.is_empty() {
        return "(empty)".to_string();
    }
    let tail: String = key
        .chars()
        .rev()
        .take(2)
        .collect::<String>()
        .chars()
        .rev()
        .collect();
    format!("***{tail}")
}

pub trait Protector {
    fn protect(&self, plain: &[u8]) -> Result<Vec<u8>, String>;
    fn unprotect(&self, blob: &[u8]) -> Result<Vec<u8>, String>;
}

const MAGIC: &[u8] = b"CTK1";
const MAX_BLOB: usize = 16 * 1024;

pub fn encode(protector: &dyn Protector, secrets: &Secrets) -> Result<Vec<u8>, String> {
    let mut out = Vec::new();
    out.extend_from_slice(MAGIC);
    write_field(&mut out, protector, secrets.deepl_key.as_bytes())?;
    write_field(&mut out, protector, secrets.azure_key.as_bytes())?;
    Ok(out)
}

pub fn decode(protector: &dyn Protector, bytes: &[u8]) -> Result<Secrets, String> {
    if bytes.len() < 4 || &bytes[..4] != MAGIC {
        return Err("The key file is not in the expected format.".into());
    }
    let mut offset = 4;
    let deepl = read_field(protector, bytes, &mut offset)?;
    let azure = read_field(protector, bytes, &mut offset)?;
    Ok(Secrets {
        deepl_key: String::from_utf8(deepl).map_err(|_| "DeepL key encoding".to_string())?,
        azure_key: String::from_utf8(azure).map_err(|_| "Azure key encoding".to_string())?,
    })
}

pub fn load_file(path: &Path, protector: &dyn Protector) -> Result<Secrets, String> {
    if !path.exists() {
        return Ok(Secrets::default());
    }
    let bytes = fs::read(path).map_err(|err| err.to_string())?;
    decode(protector, &bytes)
}

pub fn save_file(path: &Path, protector: &dyn Protector, secrets: &Secrets) -> Result<(), String> {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent).map_err(|err| err.to_string())?;
    }
    let bytes = encode(protector, secrets)?;
    let tmp = path.with_extension("bin.tmp");
    fs::write(&tmp, &bytes).map_err(|err| err.to_string())?;
    if path.exists() {
        fs::remove_file(path).map_err(|err| err.to_string())?;
    }
    fs::rename(&tmp, path).map_err(|err| err.to_string())?;
    Ok(())
}

fn write_field(out: &mut Vec<u8>, protector: &dyn Protector, plain: &[u8]) -> Result<(), String> {
    if plain.is_empty() {
        out.extend_from_slice(&0u32.to_le_bytes());
        return Ok(());
    }
    let blob = protector.protect(plain)?;
    if blob.len() > MAX_BLOB {
        return Err("The encrypted key is too long.".into());
    }
    let len = u32::try_from(blob.len()).map_err(|_| "key length".to_string())?;
    out.extend_from_slice(&len.to_le_bytes());
    out.extend_from_slice(&blob);
    Ok(())
}

fn read_field(
    protector: &dyn Protector,
    bytes: &[u8],
    offset: &mut usize,
) -> Result<Vec<u8>, String> {
    if bytes.len() < *offset + 4 {
        return Err("The key file is truncated.".into());
    }
    let len = u32::from_le_bytes(bytes[*offset..*offset + 4].try_into().unwrap()) as usize;
    *offset += 4;
    if len == 0 {
        return Ok(Vec::new());
    }
    if len > MAX_BLOB || bytes.len() < *offset + len {
        return Err("The key file is truncated.".into());
    }
    let blob = &bytes[*offset..*offset + len];
    *offset += len;
    protector.unprotect(blob)
}

#[cfg(test)]
mod tests {
    use super::*;

    struct Xor;

    impl Protector for Xor {
        fn protect(&self, plain: &[u8]) -> Result<Vec<u8>, String> {
            Ok(plain.iter().map(|byte| byte ^ 0xA5).collect())
        }
        fn unprotect(&self, blob: &[u8]) -> Result<Vec<u8>, String> {
            self.protect(blob)
        }
    }

    #[test]
    fn roundtrip_does_not_store_the_key_in_plaintext() {
        let secrets = Secrets {
            deepl_key: "unit-test-key:fx".to_string(),
            azure_key: "azure-secret".to_string(),
        };
        let bytes = encode(&Xor, &secrets).unwrap();
        assert!(!bytes
            .windows(secrets.deepl_key.len())
            .any(|window| window == secrets.deepl_key.as_bytes()));
        assert!(!bytes
            .windows(secrets.azure_key.len())
            .any(|window| window == secrets.azure_key.as_bytes()));
        assert_eq!(decode(&Xor, &bytes).unwrap(), secrets);
    }

    #[test]
    fn empty_keys_and_corrupt_header() {
        let secrets = Secrets::default();
        let bytes = encode(&Xor, &secrets).unwrap();
        assert_eq!(decode(&Xor, &bytes).unwrap(), secrets);
        assert!(decode(&Xor, b"nope").is_err());
        assert!(decode(&Xor, b"CTK1").is_err());
    }

    #[test]
    fn file_roundtrip_uses_the_protector() {
        let dir =
            std::env::temp_dir().join(format!("clip-translator-secrets-{}", std::process::id()));
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(&dir).unwrap();
        let path = dir.join("secrets.bin");
        let secrets = Secrets {
            deepl_key: "file-key:fx".into(),
            azure_key: String::new(),
        };
        save_file(&path, &Xor, &secrets).unwrap();
        let raw = fs::read(&path).unwrap();
        assert!(!raw
            .windows(b"file-key:fx".len())
            .any(|window| window == b"file-key:fx"));
        assert_eq!(load_file(&path, &Xor).unwrap(), secrets);
        let _ = fs::remove_dir_all(&dir);
    }
}
