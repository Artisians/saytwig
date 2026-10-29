//! A deliberately small HTTP/1.1 client for one authenticated loopback socket.
//! Never reconnect between identity proof and upload: a reclaimed port is not
//! equivalent to the TCP connection that proved possession of the child secret.
use crate::{integrity::MODEL_HASH, MODEL_ID, RUNTIME_VERSION};
use anyhow::{bail, ensure, Context, Result};
use hmac::{Hmac, Mac};
use sha2::Sha256;
use std::{
    collections::HashMap,
    io::{Read, Write},
    net::{SocketAddr, TcpStream},
    time::{Duration, Instant},
};

const MAX_REPLY: usize = 262_144;

struct Reply {
    status: u16,
    headers: HashMap<String, String>,
    body: Vec<u8>,
}

fn retryable(error: &std::io::Error) -> bool {
    matches!(
        error.kind(),
        std::io::ErrorKind::WouldBlock
            | std::io::ErrorKind::TimedOut
            | std::io::ErrorKind::Interrupted
    )
}

fn transfer(
    stream: &mut TcpStream,
    bytes: &[u8],
    check: &impl Fn() -> Result<()>,
    deadline: Instant,
) -> Result<()> {
    let mut cursor = 0;
    while cursor < bytes.len() {
        check()?;
        ensure!(Instant::now() < deadline, "Local speech request timed out");
        match stream.write(&bytes[cursor..bytes.len().min(cursor + 65_536)]) {
            Ok(0) => bail!("Speech connection closed before upload completed"),
            Ok(length) => cursor += length,
            Err(error) if retryable(&error) => {}
            Err(_) => bail!("Speech connection closed during upload"),
        }
    }
    Ok(())
}

fn read_exact(
    stream: &mut TcpStream,
    output: &mut [u8],
    check: &impl Fn() -> Result<()>,
    deadline: Instant,
) -> Result<()> {
    let mut cursor = 0;
    while cursor < output.len() {
        check()?;
        ensure!(Instant::now() < deadline, "Local speech response timed out");
        match stream.read(&mut output[cursor..]) {
            Ok(0) => bail!("Speech process closed its connection"),
            Ok(length) => cursor += length,
            Err(error) if retryable(&error) => {}
            Err(_) => bail!("Speech connection failed"),
        }
    }
    Ok(())
}

fn receive(
    stream: &mut TcpStream,
    check: &impl Fn() -> Result<()>,
    deadline: Instant,
) -> Result<Reply> {
    let mut header = Vec::new();
    while !header.ends_with(b"\r\n\r\n") {
        ensure!(
            header.len() < 16_384,
            "Speech response headers exceeded their limit"
        );
        let mut byte = [0];
        read_exact(stream, &mut byte, check, deadline)?;
        header.push(byte[0]);
    }
    let header = std::str::from_utf8(&header).context("Invalid speech HTTP headers")?;
    let mut lines = header.split("\r\n");
    let mut status_line = lines.next().unwrap_or_default().split(' ');
    ensure!(
        status_line.next() == Some("HTTP/1.1"),
        "Unsupported speech response protocol"
    );
    let status = status_line
        .next()
        .and_then(|value| value.parse::<u16>().ok())
        .context("Invalid speech response status")?;
    let mut headers = HashMap::new();
    for line in lines.filter(|line| !line.is_empty()) {
        let (name, value) = line
            .split_once(':')
            .context("Malformed speech HTTP header")?;
        let name = name.to_ascii_lowercase();
        ensure!(
            headers.insert(name, value.trim().to_owned()).is_none(),
            "Duplicate speech response header"
        );
    }
    ensure!(
        !headers.contains_key("transfer-encoding"),
        "Chunked speech replies are not accepted"
    );
    let length = headers
        .get("content-length")
        .and_then(|value| value.parse::<usize>().ok())
        .context("Missing speech response length")?;
    ensure!(length <= MAX_REPLY, "Speech response exceeded its limit");
    let mut body = vec![0; length];
    read_exact(stream, &mut body, check, deadline)?;
    Ok(Reply {
        status,
        headers,
        body,
    })
}

fn authenticate(port: u16, secret: &str, check: &impl Fn() -> Result<()>) -> Result<TcpStream> {
    check()?;
    let address = SocketAddr::from(([127, 0, 0, 1], port));
    let mut stream = TcpStream::connect_timeout(&address, Duration::from_millis(500))
        .context("Cannot connect to the owned speech process")?;
    stream.set_read_timeout(Some(Duration::from_millis(100)))?;
    stream.set_write_timeout(Some(Duration::from_millis(100)))?;
    let challenge = uuid::Uuid::new_v4().to_string();
    let deadline = Instant::now() + Duration::from_secs(3);
    let request = format!("GET /ready HTTP/1.1\r\nHost: 127.0.0.1:{port}\r\nX-Process-Challenge: {challenge}\r\nConnection: keep-alive\r\n\r\n");
    transfer(&mut stream, request.as_bytes(), check, deadline)?;
    let reply = receive(&mut stream, check, deadline)?;
    ensure!(reply.status == 200, "Speech process is not ready");
    let proof = reply
        .headers
        .get("x-process-proof")
        .and_then(|value| hex::decode(value).ok())
        .context("Speech endpoint did not prove its identity")?;
    let mut verifier = Hmac::<Sha256>::new_from_slice(secret.as_bytes())?;
    verifier.update(format!("{challenge}|{MODEL_HASH}").as_bytes());
    ensure!(
        verifier.verify_slice(&proof).is_ok(),
        "Speech endpoint identity failed; no credential or audio was sent"
    );
    let identity: serde_json::Value =
        serde_json::from_slice(&reply.body).context("Invalid speech identity response")?;
    ensure!(
        identity["protocol"] == 1
            && identity["model"] == MODEL_ID
            && identity["weights"] == MODEL_HASH
            && identity["runtime"] == RUNTIME_VERSION,
        "Unexpected speech model or runtime identity"
    );
    Ok(stream)
}

pub(crate) fn ready(port: u16, secret: &str, check: &impl Fn() -> Result<()>) -> Result<()> {
    drop(authenticate(port, secret, check)?);
    Ok(())
}

pub(crate) fn transcribe(
    port: u16,
    secret: &str,
    wav: &[u8],
    vocabulary: &[String],
    check: &impl Fn() -> Result<()>,
) -> Result<String> {
    let mut stream = authenticate(port, secret, check)?;
    let boundary = uuid::Uuid::new_v4().simple().to_string();
    let mut body = format!("--{boundary}\r\nContent-Disposition: form-data; name=\"vocabulary\"\r\n\r\n{}\r\n--{boundary}\r\nContent-Disposition: form-data; name=\"audio\"; filename=\"speech.wav\"\r\nContent-Type: audio/wav\r\n\r\n", serde_json::to_string(vocabulary)?).into_bytes();
    body.extend_from_slice(wav);
    body.extend_from_slice(format!("\r\n--{boundary}--\r\n").as_bytes());
    ensure!(
        body.len() <= 16_000 * 120 * 2 + 16_384,
        "Recording form exceeded its limit"
    );
    let deadline = Instant::now() + Duration::from_secs(180);
    let header = format!("POST /transcribe HTTP/1.1\r\nHost: 127.0.0.1:{port}\r\nAuthorization: Bearer {secret}\r\nContent-Type: multipart/form-data; boundary={boundary}\r\nContent-Length: {}\r\nConnection: close\r\n\r\n", body.len());
    transfer(&mut stream, header.as_bytes(), check, deadline)?;
    transfer(&mut stream, &body, check, deadline)?;
    let reply = receive(&mut stream, check, deadline)?;
    ensure!(
        reply.status == 200,
        "Local speech decoding failed (HTTP {})",
        reply.status
    );
    let decoded: serde_json::Value =
        serde_json::from_slice(&reply.body).context("Malformed speech result")?;
    let text = decoded["text"]
        .as_str()
        .context("Speech result did not contain text")?;
    ensure!(
        text.len() <= 131_072 && !text.contains('\0'),
        "Invalid speech result"
    );
    Ok(text.trim().to_owned())
}
