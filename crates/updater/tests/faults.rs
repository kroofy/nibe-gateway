use nibe_updater::{
    artifact::{self, append, PI_TARGET},
    Checkpoint, NoFault, Phase, Result, ServiceManager, Updater,
};
use std::{fs, path::Path};
const SECRET: [u8; 32] = [17; 32];
fn public() -> [u8; 32] {
    ed25519_dalek::SigningKey::from_bytes(&SECRET)
        .verifying_key()
        .to_bytes()
}
fn release(version: &str, target: &str) -> Vec<u8> {
    let mut payload = tar::Builder::new(Vec::new());
    append(&mut payload, "nibe-gateway", b"binary").unwrap();
    append(&mut payload, "admin/index.html", b"admin").unwrap();
    append(&mut payload, "admin/assets/app.js", b"app").unwrap();
    artifact::sign(&payload.into_inner().unwrap(), version, target, &SECRET).unwrap()
}
fn factory(root: &Path) {
    let verified = artifact::verify(&release("0.1.0", PI_TARGET), &public(), PI_TARGET).unwrap();
    let slot = root.join("releases/slot-a");
    fs::create_dir_all(&slot).unwrap();
    for (name, bytes) in verified.files {
        let path = slot.join(name);
        fs::create_dir_all(path.parent().unwrap()).unwrap();
        fs::write(path, bytes).unwrap();
    }
    fs::write(slot.join("manifest.json"), verified.manifest_bytes).unwrap();
    Updater::open(root, public(), PI_TARGET)
        .unwrap()
        .initialize()
        .unwrap();
}
#[derive(Default)]
struct Fake {
    restarts: usize,
    fail_next: bool,
}
impl ServiceManager for Fake {
    fn restart(&mut self) -> Result<()> {
        self.restarts += 1;
        Ok(())
    }
    fn ready(&mut self, version: &str, id: &str) -> Result<bool> {
        assert!(!version.is_empty());
        assert_eq!(id.len(), 64);
        Ok(!std::mem::take(&mut self.fail_next))
    }
}
struct Cut {
    at: usize,
    count: usize,
}
impl Checkpoint for Cut {
    fn hit(&mut self, _: &str) -> Result<()> {
        self.count += 1;
        if self.count == self.at {
            Err(nibe_updater::fail("power loss"))
        } else {
            Ok(())
        }
    }
}
#[test]
fn rejects_corruption_signature_and_compatibility_without_touching_slots() {
    let dir = tempfile::tempdir().unwrap();
    factory(dir.path());
    let mut updater = Updater::open(dir.path(), public(), PI_TARGET).unwrap();
    let valid = release("0.2.0", PI_TARGET);
    let mut outer = tar::Archive::new(valid.as_slice());
    let mut entries = vec![];
    use std::io::Read;
    for entry in outer.entries().unwrap() {
        let mut entry = entry.unwrap();
        let name = entry.path().unwrap().to_string_lossy().into_owned();
        let mut bytes = vec![];
        entry.read_to_end(&mut bytes).unwrap();
        entries.push((name, bytes));
    }
    for corrupt in ["payload.tar", "manifest.sig", "manifest.json"] {
        let mut builder = tar::Builder::new(Vec::new());
        for (name, bytes) in &entries {
            let mut bytes = bytes.clone();
            if name == corrupt {
                bytes[0] ^= 1;
            }
            append(&mut builder, name, &bytes).unwrap();
        }
        assert!(updater
            .stage(&builder.into_inner().unwrap(), &mut NoFault)
            .is_err());
    }
    assert!(updater
        .stage(&release("0.2.0", "wrong-target"), &mut NoFault)
        .is_err());
    // Correctly signed but incompatible config-version range.
    let mut builder = tar::Builder::new(Vec::new());
    let mut manifest: serde_json::Value = serde_json::from_slice(
        &entries
            .iter()
            .find(|(n, _)| n == "manifest.json")
            .unwrap()
            .1,
    )
    .unwrap();
    manifest["config_schema_min"] = 3.into();
    manifest["config_schema_max"] = 3.into();
    let manifest = serde_json::to_vec(&manifest).unwrap();
    use ed25519_dalek::Signer;
    let signature = ed25519_dalek::SigningKey::from_bytes(&SECRET).sign(&manifest);
    append(&mut builder, "manifest.json", &manifest).unwrap();
    append(&mut builder, "manifest.sig", &signature.to_bytes()).unwrap();
    append(
        &mut builder,
        "payload.tar",
        &entries.iter().find(|(n, _)| n == "payload.tar").unwrap().1,
    )
    .unwrap();
    assert!(updater
        .stage(&builder.into_inner().unwrap(), &mut NoFault)
        .is_err());
    assert!(!dir.path().join("releases/slot-b").exists());
    assert_eq!(updater.state.phase, Phase::Idle);
}
#[test]
fn successful_activation_manual_rollback_and_failed_readiness() {
    let dir = tempfile::tempdir().unwrap();
    factory(dir.path());
    let mut service = Fake::default();
    let mut updater = Updater::open(dir.path(), public(), PI_TARGET).unwrap();
    updater
        .stage(&release("0.2.0", PI_TARGET), &mut NoFault)
        .unwrap();
    updater.activate(&mut service, &mut NoFault).unwrap();
    assert_eq!(updater.state.phase, Phase::Complete);
    assert_eq!(
        fs::read_link(dir.path().join("current")).unwrap(),
        Path::new("releases/slot-b")
    );
    updater.rollback(&mut service, &mut NoFault).unwrap();
    assert_eq!(updater.state.phase, Phase::RolledBack);
    updater
        .stage(&release("0.3.0", PI_TARGET), &mut NoFault)
        .unwrap();
    service.fail_next = true;
    updater.activate(&mut service, &mut NoFault).unwrap();
    assert_eq!(updater.state.phase, Phase::RolledBack);
    assert_eq!(
        fs::read_link(dir.path().join("current")).unwrap(),
        Path::new("releases/slot-a")
    );
    assert_eq!(service.restarts, 4);
}
#[test]
fn power_loss_at_every_staging_and_activation_boundary() {
    // Intent, remove inactive, three payload files, sync complete, staged.
    for cut in 1..=7 {
        let dir = tempfile::tempdir().unwrap();
        factory(dir.path());
        let mut updater = Updater::open(dir.path(), public(), PI_TARGET).unwrap();
        assert!(updater
            .stage(&release("0.2.0", PI_TARGET), &mut Cut { at: cut, count: 0 })
            .is_err());
        drop(updater);
        let mut updater = Updater::open(dir.path(), public(), PI_TARGET).unwrap();
        updater.recover(&mut Fake::default()).unwrap();
        assert_eq!(
            fs::read_link(dir.path().join("current")).unwrap(),
            Path::new("releases/slot-a")
        );
        assert!(matches!(updater.state.phase, Phase::Failed | Phase::Staged));
        if updater.state.phase == Phase::Failed {
            assert!(!dir.path().join("releases/slot-b").exists());
        }
        updater
            .stage(&release("0.3.0", PI_TARGET), &mut NoFault)
            .unwrap();
    }
    for cut in 1..=6 {
        let dir = tempfile::tempdir().unwrap();
        factory(dir.path());
        let mut updater = Updater::open(dir.path(), public(), PI_TARGET).unwrap();
        updater
            .stage(&release("0.2.0", PI_TARGET), &mut NoFault)
            .unwrap();
        assert!(updater
            .activate(&mut Fake::default(), &mut Cut { at: cut, count: 0 })
            .is_err());
        drop(updater);
        let mut updater = Updater::open(dir.path(), public(), PI_TARGET).unwrap();
        updater.recover(&mut Fake::default()).unwrap();
        assert_eq!(
            updater.state.phase,
            if cut == 6 {
                Phase::Complete
            } else {
                Phase::RolledBack
            }
        );
        updater.recover(&mut Fake::default()).unwrap();
    }
}
#[test]
fn power_loss_during_rollback_is_idempotently_recovered() {
    for cut in 1..=4 {
        let dir = tempfile::tempdir().unwrap();
        factory(dir.path());
        let mut service = Fake::default();
        let mut updater = Updater::open(dir.path(), public(), PI_TARGET).unwrap();
        updater
            .stage(&release("0.2.0", PI_TARGET), &mut NoFault)
            .unwrap();
        updater.activate(&mut service, &mut NoFault).unwrap();
        assert!(updater
            .rollback(&mut service, &mut Cut { at: cut, count: 0 })
            .is_err());
        drop(updater);
        let mut updater = Updater::open(dir.path(), public(), PI_TARGET).unwrap();
        updater.recover(&mut service).unwrap();
        assert_eq!(updater.state.phase, Phase::RolledBack);
        assert_eq!(
            fs::read_link(dir.path().join("current")).unwrap(),
            Path::new("releases/slot-a")
        );
    }
}
#[test]
fn malformed_state_is_not_reset_and_concurrent_updater_is_refused() {
    let dir = tempfile::tempdir().unwrap();
    factory(dir.path());
    let updater = Updater::open(dir.path(), public(), PI_TARGET).unwrap();
    assert!(Updater::open(dir.path(), public(), PI_TARGET).is_err());
    drop(updater);
    fs::remove_file(dir.path().join("update-state/state.json")).unwrap();
    assert!(Updater::open(dir.path(), public(), PI_TARGET).is_err());
    fs::write(dir.path().join("update-state/state.json"), b"broken").unwrap();
    assert!(Updater::open(dir.path(), public(), PI_TARGET).is_err());
}
#[test]
fn readiness_requires_json_semantics_and_release_identity() {
    use std::{
        io::{Read, Write},
        net::TcpListener,
    };
    for body in [
        r#"{"status":"ok"}"#,
        r#"{"status":"ok","ready":false,"version":"0.2.0","release_id":"abc"}"#,
        r#"{"status":"ok","ready":true,"version":"0.1.0","release_id":"abc"}"#,
        r#"{"status":"ok","ready":true,"version":"0.2.0","release_id":"wrong"}"#,
        r#"{"status":"ok","ready":true,"version":"0.2.0","release_id":"abc"}"#,
    ] {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let addr = listener.local_addr().unwrap();
        let body = body.to_string();
        let good = body.contains("true") && body.contains("0.2.0") && body.contains("abc");
        let task = std::thread::spawn(move || {
            let (mut stream, _) = listener.accept().unwrap();
            let mut buf = [0; 512];
            assert!(stream.read(&mut buf).unwrap() > 0);
            write!(
                stream,
                "HTTP/1.0 200 OK\r\nContent-Length: {}\r\n\r\n{}",
                body.len(),
                body
            )
            .unwrap();
        });
        assert_eq!(nibe_updater::service::probe(addr, "0.2.0", "abc"), good);
        task.join().unwrap();
    }
}

#[test]
fn signed_archives_still_cannot_escape_or_alias_slot_files() {
    for case in 0..4 {
        let mut payload = tar::Builder::new(Vec::new());
        append(&mut payload, "nibe-gateway", b"binary").unwrap();
        append(&mut payload, "admin/index.html", b"admin").unwrap();
        match case {
            0 => {
                let mut header = tar::Header::new_gnu();
                header.set_entry_type(tar::EntryType::Symlink);
                header.set_size(0);
                header.set_mode(0o777);
                header.set_link_name("/etc/shadow").unwrap();
                header.set_cksum();
                payload
                    .append_data(&mut header, "admin/link", &b""[..])
                    .unwrap();
            }
            1 => append(&mut payload, "admin/index.html", b"duplicate").unwrap(),
            2 => {
                append(&mut payload, "admin/assets", b"not a directory").unwrap();
                append(&mut payload, "admin/assets/app.js", b"conflict").unwrap();
            }
            _ => append(
                &mut payload,
                "config.json",
                b"must not replace persistent config",
            )
            .unwrap(),
        }
        let release =
            artifact::sign(&payload.into_inner().unwrap(), "0.2.0", PI_TARGET, &SECRET).unwrap();
        assert!(artifact::verify(&release, &public(), PI_TARGET).is_err());
    }
}

#[test]
fn downgrade_requires_confirmation_and_repeated_rollback_is_rejected() {
    let dir = tempfile::tempdir().unwrap();
    factory(dir.path());
    let mut updater = Updater::open(dir.path(), public(), PI_TARGET).unwrap();
    // SemVer build metadata does not change precedence or require downgrade consent.
    updater
        .stage(&release("0.1.0+build", PI_TARGET), &mut NoFault)
        .unwrap();
    let older = release("0.0.9", PI_TARGET);
    assert!(updater.stage(&older, &mut NoFault).is_err());
    assert_eq!(updater.state.phase, Phase::Staged);
    updater.stage_confirmed(&older, true, &mut NoFault).unwrap();
    updater
        .activate(&mut Fake::default(), &mut NoFault)
        .unwrap();
    updater
        .rollback(&mut Fake::default(), &mut NoFault)
        .unwrap();
    assert!(updater.prepare_rollback().is_err());
}
#[test]
fn probation_crash_recovers_previous_slot() {
    let dir = tempfile::tempdir().unwrap();
    factory(dir.path());
    let mut updater = Updater::open(dir.path(), public(), PI_TARGET).unwrap();
    updater
        .stage(&release("0.2.0", PI_TARGET), &mut NoFault)
        .unwrap();
    struct CutProbation;
    impl Checkpoint for CutProbation {
        fn hit(&mut self, name: &str) -> Result<()> {
            if name == "probation" {
                Err(nibe_updater::fail("boot loss"))
            } else {
                Ok(())
            }
        }
    }
    assert!(updater
        .activate(&mut Fake::default(), &mut CutProbation)
        .is_err());
    assert_eq!(updater.state.phase, Phase::Probation);
    drop(updater);
    let mut updater = Updater::open(dir.path(), public(), PI_TARGET).unwrap();
    updater.recover(&mut Fake::default()).unwrap();
    assert_eq!(updater.state.phase, Phase::RolledBack);
}
