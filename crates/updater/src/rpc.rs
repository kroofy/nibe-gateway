//! Length-prefixed local protocol; filesystem permissions are the privilege boundary.
use crate::{artifact::MAX_ARTIFACT, fail, Result};
use serde::{Deserialize, Serialize};
use std::{
    io::{Read, Write},
    os::unix::net::UnixStream,
    path::Path,
    time::Duration,
};
#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Action {
    Status,
    Upload,
    UploadDowngrade,
    Activate,
    Rollback,
    Restart,
}
#[derive(Debug, Serialize, Deserialize)]
pub struct Reply {
    pub ok: bool,
    pub status: serde_json::Value,
}
fn send(stream: &mut UnixStream, bytes: &[u8]) -> Result<()> {
    stream.write_all(&(bytes.len() as u32).to_be_bytes())?;
    stream.write_all(bytes)?;
    Ok(())
}
fn receive(stream: &mut UnixStream, max: usize) -> Result<Vec<u8>> {
    let mut len = [0; 4];
    stream.read_exact(&mut len)?;
    let len = u32::from_be_bytes(len) as usize;
    if len > max {
        return Err(fail("RPC size limit"));
    }
    let mut bytes = vec![0; len];
    stream.read_exact(&mut bytes)?;
    Ok(bytes)
}
pub fn request(path: &Path, action: Action, bytes: &[u8]) -> Result<Reply> {
    let mut stream = UnixStream::connect(path)?;
    stream.set_read_timeout(Some(Duration::from_secs(120)))?;
    stream.set_write_timeout(Some(Duration::from_secs(30)))?;
    send(&mut stream, &serde_json::to_vec(&action)?)?;
    if matches!(action, Action::Upload | Action::UploadDowngrade) {
        send(&mut stream, bytes)?;
    }
    Ok(serde_json::from_slice(&receive(&mut stream, 65536)?)?)
}
pub fn read_action(stream: &mut UnixStream) -> Result<Action> {
    stream.set_read_timeout(Some(Duration::from_secs(30)))?;
    stream.set_write_timeout(Some(Duration::from_secs(5)))?;
    Ok(serde_json::from_slice(&receive(stream, 64)?)?)
}
pub fn read_upload(stream: &mut UnixStream) -> Result<Vec<u8>> {
    receive(stream, MAX_ARTIFACT)
}
pub fn reply(stream: &mut UnixStream, ok: bool, status: serde_json::Value) -> Result<()> {
    send(stream, &serde_json::to_vec(&Reply { ok, status })?)
}
