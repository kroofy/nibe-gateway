use nibe_updater::{
    artifact::{self, append},
    Result,
};
use std::{
    fs,
    io::Read,
    os::unix::fs::{OpenOptionsExt, PermissionsExt},
    path::Path,
};
fn main() -> Result<()> {
    let args: Vec<String> = std::env::args().skip(1).collect();
    if args.first().map(String::as_str) == Some("keygen") && args.len() == 3 {
        let mut secret = [0; 32];
        fs::File::open("/dev/urandom")?.read_exact(&mut secret)?;
        let mut key = fs::OpenOptions::new()
            .create_new(true)
            .write(true)
            .mode(0o600)
            .open(&args[1])?;
        use std::io::Write;
        key.write_all(&secret)?;
        key.sync_all()?;
        let public = ed25519_dalek::SigningKey::from_bytes(&secret).verifying_key();
        let mut out = fs::OpenOptions::new()
            .create_new(true)
            .write(true)
            .mode(0o644)
            .open(&args[2])?;
        out.write_all(&public.to_bytes())?;
        out.sync_all()?;
        return Ok(());
    }
    let command = args.first().map(String::as_str);
    if !(command == Some("sign") && args.len() == 7
        || command == Some("os-sign") && args.len() == 6)
    {
        println!("nibe-release keygen PRIVATE PUBLIC\nnibe-release sign PRIVATE VERSION TARGET GATEWAY ADMIN_DIR OUTPUT.tar\nnibe-release os-sign PRIVATE VERSION ROOTFS_IMG BOOT_DIR OUTPUT.tar");
        return Ok(());
    }
    if fs::metadata(&args[1])?.permissions().mode() & 0o077 != 0 {
        return Err(nibe_updater::fail("private key permissions must be 0600"));
    }
    let secret: [u8; 32] = fs::read(&args[1])?
        .try_into()
        .map_err(|_| nibe_updater::fail("private key must be 32 bytes"))?;
    if command == Some("os-sign") {
        let manifest = nibe_updater::os::sign_bundle(
            &secret,
            &args[2],
            Path::new(&args[3]),
            Path::new(&args[4]),
            Path::new(&args[5]),
        )?;
        println!("{}", serde_json::to_string(&manifest)?);
        return Ok(());
    }
    let mut payload = tar::Builder::new(Vec::new());
    append(&mut payload, "nibe-gateway", &fs::read(&args[4])?)?;
    fn walk(builder: &mut tar::Builder<Vec<u8>>, base: &Path, path: &Path) -> Result<()> {
        let mut entries = fs::read_dir(path)?.collect::<std::io::Result<Vec<_>>>()?;
        entries.sort_by_key(|e| e.file_name());
        for entry in entries {
            if entry.file_type()?.is_symlink() {
                return Err(nibe_updater::fail("symlinks forbidden"));
            }
            if entry.file_type()?.is_dir() {
                walk(builder, base, &entry.path())?;
            } else {
                append(
                    builder,
                    &format!(
                        "admin/{}",
                        entry
                            .path()
                            .strip_prefix(base)?
                            .to_str()
                            .ok_or_else(|| nibe_updater::fail("invalid filename"))?
                    ),
                    &fs::read(entry.path())?,
                )?;
            }
        }
        Ok(())
    }
    walk(&mut payload, Path::new(&args[5]), Path::new(&args[5]))?;
    let release = artifact::sign(&payload.into_inner()?, &args[2], &args[3], &secret)?;
    // Apply precisely the verifier's structural/size policy before publishing.
    artifact::verify(
        &release,
        &ed25519_dalek::SigningKey::from_bytes(&secret)
            .verifying_key()
            .to_bytes(),
        &args[3],
    )?;
    fs::write(&args[6], release)?;
    Ok(())
}
