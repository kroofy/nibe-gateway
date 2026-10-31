//! Version 1: outer uncompressed tar containing exactly manifest.json,
//! manifest.sig (raw Ed25519 over the exact manifest bytes), payload.tar.
use crate::{fail, Result};
use ed25519_dalek::{Signature, Signer, SigningKey, VerifyingKey};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::{
    collections::BTreeMap,
    io::Read,
    path::{Component, Path},
};

pub const MAX_ARTIFACT: usize = 48 * 1024 * 1024;
pub const COMPAT: &str = "nibe-gateway-app-v1";
pub const PI_TARGET: &str = "arm-unknown-linux-gnueabihf";
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Manifest {
    pub format: u32,
    pub version: String,
    pub compatibility: String,
    pub target: String,
    pub config_schema_min: u32,
    pub config_schema_max: u32,
    pub sha256: String,
}
pub fn digest(bytes: &[u8]) -> String {
    format!("{:x}", Sha256::digest(bytes))
}
fn entries(bytes: &[u8], payload: bool) -> Result<BTreeMap<String, Vec<u8>>> {
    let mut files = BTreeMap::new();
    let mut total = 0usize;
    for item in tar::Archive::new(bytes).entries()? {
        let mut item = item?;
        let path = item.path()?.into_owned();
        let name = path
            .to_str()
            .ok_or_else(|| fail("non UTF-8 path"))?
            .to_owned();
        if !item.header().entry_type().is_file()
            || path
                .components()
                .any(|c| !matches!(c, Component::Normal(_)))
            || path
                .components()
                .map(|c| c.as_os_str().to_string_lossy())
                .collect::<Vec<_>>()
                .join("/")
                != name
            || name.contains('\\')
            || name.len() > 240
            || if payload {
                !(name == "nibe-gateway" || name.starts_with("admin/"))
            } else {
                !matches!(
                    name.as_str(),
                    "manifest.json" | "manifest.sig" | "payload.tar"
                )
            }
        {
            return Err(fail("unsafe archive entry"));
        }
        total = total
            .checked_add(usize::try_from(item.size())?)
            .ok_or_else(|| fail("size overflow"))?;
        if total > MAX_ARTIFACT || files.len() >= 2048 {
            return Err(fail("archive limit"));
        }
        let mut data = Vec::new();
        item.read_to_end(&mut data)?;
        if files.insert(name, data).is_some() {
            return Err(fail("duplicate archive entry"));
        }
    }
    Ok(files)
}
pub struct Verified {
    pub manifest: Manifest,
    pub manifest_bytes: Vec<u8>,
    pub signature: Vec<u8>,
    pub files: BTreeMap<String, Vec<u8>>,
}
pub fn verify(bytes: &[u8], public: &[u8; 32], target: &str) -> Result<Verified> {
    if bytes.len() > MAX_ARTIFACT {
        return Err(fail("artifact too large"));
    }
    let mut outer = entries(bytes, false)?;
    let manifest_bytes = outer
        .remove("manifest.json")
        .ok_or_else(|| fail("manifest missing"))?;
    let signature = outer
        .remove("manifest.sig")
        .ok_or_else(|| fail("signature missing"))?;
    let payload = outer
        .remove("payload.tar")
        .ok_or_else(|| fail("payload missing"))?;
    VerifyingKey::from_bytes(public)?
        .verify_strict(&manifest_bytes, &Signature::from_slice(&signature)?)?;
    let manifest: Manifest = serde_json::from_slice(&manifest_bytes)?;
    semver::Version::parse(&manifest.version)?;
    if manifest.format != 1
        || manifest.compatibility != COMPAT
        || manifest.target != target
        || manifest.config_schema_min > 2
        || manifest.config_schema_max < 2
        || manifest.sha256 != digest(&payload)
    {
        return Err(fail("incompatible release or checksum mismatch"));
    }
    let files = entries(&payload, true)?;
    if !files.contains_key("nibe-gateway") || !files.contains_key("admin/index.html") {
        return Err(fail("gateway or admin/index.html missing"));
    }
    // A file must never also be a parent directory of another file.
    for path in files.keys() {
        for parent in Path::new(path).ancestors().skip(1) {
            if parent.to_str().is_some_and(|p| files.contains_key(p)) {
                return Err(fail("conflicting archive paths"));
            }
        }
    }
    Ok(Verified {
        manifest,
        manifest_bytes,
        signature,
        files,
    })
}
pub fn append(builder: &mut tar::Builder<Vec<u8>>, name: &str, bytes: &[u8]) -> Result<()> {
    let mut header = tar::Header::new_gnu();
    header.set_size(bytes.len() as u64);
    header.set_mode(0o644);
    header.set_cksum();
    builder.append_data(&mut header, name, bytes)?;
    Ok(())
}
pub fn sign(payload: &[u8], version: &str, target: &str, secret: &[u8; 32]) -> Result<Vec<u8>> {
    semver::Version::parse(version)?;
    let manifest = Manifest {
        format: 1,
        version: version.into(),
        compatibility: COMPAT.into(),
        target: target.into(),
        config_schema_min: 2,
        config_schema_max: 2,
        sha256: digest(payload),
    };
    let bytes = serde_json::to_vec(&manifest)?;
    let signature = SigningKey::from_bytes(secret).sign(&bytes);
    let mut archive = tar::Builder::new(Vec::new());
    append(&mut archive, "manifest.json", &bytes)?;
    append(&mut archive, "manifest.sig", &signature.to_bytes())?;
    append(&mut archive, "payload.tar", payload)?;
    Ok(archive.into_inner()?)
}
