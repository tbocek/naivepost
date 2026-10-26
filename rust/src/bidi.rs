//! F6.1 §2's browser driver: headless Firefox over WebDriver BiDi, as `spec/09-llm-and-tools.md` §2 says and
//! the prototype does (`go/websearch.go`).
//!
//! The first port launched `firefox --headless --dump-dom <url>`. That flag is Chrome's: Firefox (140 ESR and
//! 158 both) ignores it, prints nothing, and keeps running until it is killed, so every search ended "the page
//! did not finish" and the model never had a web tool. This is the protocol Firefox does speak: a WebSocket on
//! `--remote-debugging-port`, `session.new`, `browsingContext.navigate`, `script.evaluate`.
//!
//! One browser per call, launched on a throwaway profile and killed on drop (§2: "one headless browser launched
//! and killed per tool call"). The WebSocket is the smallest client BiDi needs, written here rather than taken
//! as a dependency: a handshake, masked text frames out, text/ping/close frames in (RFC 6455).

use std::io::{BufRead, BufReader, Read, Write};
use std::net::TcpStream;
use std::path::{Path, PathBuf};
use std::process::{Child, Command, Stdio};
use std::time::{Duration, Instant};

use serde_json::{json, Value};

/// A headless Firefox and its BiDi session. Dropping it kills the browser and removes its profile.
pub struct Browser {
    child: Child,
    profile: PathBuf,
    socket: Socket,
    tab: String,
    next_id: u64,
}

impl Drop for Browser {
    fn drop(&mut self) {
        // Kill, not a polite quit: a private window on a throwaway profile holds nothing worth the seconds a
        // shutdown takes. Both answers are about a process that is going away either way.
        let _ = self.child.kill();
        let _ = self.child.wait();
        let _ = std::fs::remove_dir_all(&self.profile);
    }
}

impl Browser {
    /// Launch `firefox` and open its BiDi session, within `deadline`. The first page is navigated to
    /// afterwards ([`Browser::navigate`]), because a URL on the command line gives no signal for when it has
    /// loaded.
    ///
    /// The port is Firefox's own choice (`--remote-debugging-port=0`), read back from the
    /// `WebDriverBiDiServer.json` it writes into this launch's profile. A port picked here (§2's "from 9223
    /// upward") is only this process's guess: a second naivepost, or a test binary beside another, picks the
    /// same one, and the session is then asked of the OTHER browser ("Maximum number of active sessions").
    pub fn launch(firefox: &Path, deadline: Instant) -> Result<Browser, String> {
        static LAUNCHES: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);
        let n = LAUNCHES.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
        let profile =
            std::env::temp_dir().join(format!("naivepost-web-{}-{n}", std::process::id()));
        let _ = std::fs::remove_dir_all(&profile);
        std::fs::create_dir_all(&profile)
            .map_err(|why| format!("no profile folder {}: {why}", profile.display()))?;
        let child = Command::new(firefox)
            .args(["--headless", "--no-remote", "--new-instance", "--profile"])
            .arg(&profile)
            .arg("--remote-debugging-port=0")
            .arg("about:blank")
            .env("MOZ_HEADLESS", "1")
            .stdin(Stdio::null())
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .spawn()
            .map_err(|why| format!("could not start {}: {why}", firefox.display()))?;
        // The guard exists from here on, so every early return below kills the browser.
        let mut guard = Guard {
            child: Some(child),
            profile: profile.clone(),
        };
        let server = profile.join("WebDriverBiDiServer.json");
        let port = loop {
            let written = std::fs::read_to_string(&server)
                .ok()
                .and_then(|text| serde_json::from_str::<Value>(&text).ok())
                .and_then(|json| json["ws_port"].as_u64());
            if let Some(port) = written.filter(|port| *port > 0) {
                break port;
            }
            // A browser that has already exited will never open its port: say so now rather than waiting
            // out the boot window.
            if let Some(Ok(Some(status))) = guard.child.as_mut().map(|c| c.try_wait()) {
                return Err(format!(
                    "{} exited ({status}) before opening its port",
                    firefox.display()
                ));
            }
            if Instant::now() >= deadline {
                return Err("timed out: firefox did not open its debugging port".to_string());
            }
            std::thread::sleep(Duration::from_millis(100));
        };
        let addr = format!("127.0.0.1:{port}");
        let stream = TcpStream::connect_timeout(
            &addr.parse().map_err(|why| format!("{addr}: {why}"))?,
            Duration::from_secs(2),
        )
        .map_err(|why| format!("firefox bidi on {addr}: {why}"))?;
        let socket = Socket::open(stream, &addr, "/session", deadline)?;
        let mut browser = Browser {
            child: guard.child.take().expect("the child is held until here"),
            profile,
            socket,
            tab: String::new(),
            next_id: 0,
        };
        browser.send("session.new", json!({"capabilities": {}}), deadline)?;
        let tree = browser.send("browsingContext.getTree", json!({}), deadline)?;
        browser.tab = tree["contexts"][0]["context"]
            .as_str()
            .ok_or("firefox bidi: no browsing context")?
            .to_string();
        Ok(browser)
    }

    /// Load `url` and wait until it has loaded or `deadline` passes. Answers whether the load completed: a
    /// search reads a page still loading as it stands (the results are drawn before the last tracker
    /// arrives), a read of a page that never arrived is a failure.
    pub fn navigate(&mut self, url: &str, deadline: Instant) -> Result<bool, String> {
        match self.send(
            "browsingContext.navigate",
            json!({"context": self.tab, "url": url, "wait": "complete"}),
            deadline,
        ) {
            Ok(_) => Ok(true),
            Err(why) if why.contains("timed out") => Ok(false),
            Err(why) => Err(why),
        }
    }

    /// Run `expression` in the page and return its string value ("" for anything that is not a string).
    pub fn eval(&mut self, expression: &str, deadline: Instant) -> Result<String, String> {
        let answer = self.send(
            "script.evaluate",
            json!({"expression": expression, "target": {"context": self.tab}, "awaitPromise": false}),
            deadline,
        )?;
        Ok(answer["result"]["value"]
            .as_str()
            .unwrap_or_default()
            .to_string())
    }

    /// One BiDi command and its answer. Events (messages with no `id`) are nobody's and are skipped.
    fn send(&mut self, method: &str, params: Value, deadline: Instant) -> Result<Value, String> {
        self.next_id += 1;
        let id = self.next_id;
        self.socket.send_text(
            &json!({"id": id, "method": method, "params": params}).to_string(),
            deadline,
        )?;
        loop {
            let text = self.socket.read_text(deadline)?;
            let Ok(message) = serde_json::from_str::<Value>(&text) else {
                continue;
            };
            if message["id"].as_u64() != Some(id) {
                continue;
            }
            if message["type"] == "error" {
                return Err(format!(
                    "{method}: {}: {}",
                    message["error"].as_str().unwrap_or("error"),
                    message["message"].as_str().unwrap_or_default()
                ));
            }
            return Ok(message["result"].clone());
        }
    }
}

/// The browser before its session is up: killed on drop like [`Browser`], so a failed launch leaves nothing.
struct Guard {
    child: Option<Child>,
    profile: PathBuf,
}

impl Drop for Guard {
    fn drop(&mut self) {
        if let Some(mut child) = self.child.take() {
            let _ = child.kill();
            let _ = child.wait();
            let _ = std::fs::remove_dir_all(&self.profile);
        }
    }
}

/// A client WebSocket: just what BiDi needs.
struct Socket {
    stream: TcpStream,
    reader: BufReader<TcpStream>,
}

impl Socket {
    /// The opening handshake (RFC 6455 §4.1), the answer checked for `101` and the right accept key.
    fn open(
        stream: TcpStream,
        host: &str,
        path: &str,
        deadline: Instant,
    ) -> Result<Socket, String> {
        let key = base64(&nonce16());
        let mut socket = Socket {
            reader: BufReader::new(stream.try_clone().map_err(|why| why.to_string())?),
            stream,
        };
        socket.limit(deadline)?;
        let request = format!(
            "GET {path} HTTP/1.1\r\nHost: {host}\r\nUpgrade: websocket\r\nConnection: Upgrade\r\n\
             Sec-WebSocket-Key: {key}\r\nSec-WebSocket-Version: 13\r\n\r\n"
        );
        socket
            .stream
            .write_all(request.as_bytes())
            .map_err(|why| format!("firefox bidi handshake: {why}"))?;
        let mut status = String::new();
        socket
            .reader
            .read_line(&mut status)
            .map_err(|why| format!("firefox bidi handshake: {why}"))?;
        if !status.contains(" 101 ") {
            return Err(format!("firefox bidi handshake answered {}", status.trim()));
        }
        let want = accept_for(&key);
        let mut accepted = false;
        loop {
            let mut line = String::new();
            socket
                .reader
                .read_line(&mut line)
                .map_err(|why| format!("firefox bidi handshake: {why}"))?;
            let line = line.trim_end();
            if line.is_empty() {
                break;
            }
            if let Some((name, value)) = line.split_once(':') {
                if name.eq_ignore_ascii_case("sec-websocket-accept") && value.trim() == want {
                    accepted = true;
                }
            }
        }
        if !accepted {
            return Err("firefox bidi handshake: the accept key did not match".to_string());
        }
        Ok(socket)
    }

    /// Bound the next read and write by `deadline`.
    fn limit(&mut self, deadline: Instant) -> Result<(), String> {
        let left = deadline
            .checked_duration_since(Instant::now())
            .filter(|left| !left.is_zero())
            .ok_or("timed out")?;
        self.stream
            .set_read_timeout(Some(left))
            .and_then(|_| self.stream.set_write_timeout(Some(left)))
            .map_err(|why| why.to_string())
    }

    /// One text message, masked as a client must (RFC 6455 §5.3).
    fn send_text(&mut self, text: &str, deadline: Instant) -> Result<(), String> {
        self.send_frame(0x1, text.as_bytes(), deadline)
    }

    fn send_frame(&mut self, opcode: u8, payload: &[u8], deadline: Instant) -> Result<(), String> {
        self.limit(deadline)?;
        let mut frame = vec![0x80 | opcode];
        let len = payload.len();
        if len < 126 {
            frame.push(0x80 | len as u8);
        } else if len <= u16::MAX as usize {
            frame.push(0x80 | 126);
            frame.extend_from_slice(&(len as u16).to_be_bytes());
        } else {
            frame.push(0x80 | 127);
            frame.extend_from_slice(&(len as u64).to_be_bytes());
        }
        let mask: [u8; 4] = nonce16()[..4].try_into().expect("four bytes");
        frame.extend_from_slice(&mask);
        frame.extend(payload.iter().enumerate().map(|(i, b)| b ^ mask[i % 4]));
        self.stream
            .write_all(&frame)
            .map_err(|why| format!("firefox bidi: {why}"))
    }

    /// The next whole text message: fragments joined, pings answered, a close reported.
    fn read_text(&mut self, deadline: Instant) -> Result<String, String> {
        let mut message = Vec::new();
        loop {
            self.limit(deadline)?;
            let mut head = [0u8; 2];
            self.read_exact(&mut head)?;
            let fin = head[0] & 0x80 != 0;
            let opcode = head[0] & 0x0f;
            let masked = head[1] & 0x80 != 0;
            let len = match head[1] & 0x7f {
                126 => {
                    let mut ext = [0u8; 2];
                    self.read_exact(&mut ext)?;
                    u16::from_be_bytes(ext) as usize
                }
                127 => {
                    let mut ext = [0u8; 8];
                    self.read_exact(&mut ext)?;
                    u64::from_be_bytes(ext) as usize
                }
                short => short as usize,
            };
            let mut mask = [0u8; 4];
            if masked {
                self.read_exact(&mut mask)?;
            }
            let mut payload = vec![0u8; len];
            self.read_exact(&mut payload)?;
            if masked {
                for (i, byte) in payload.iter_mut().enumerate() {
                    *byte ^= mask[i % 4];
                }
            }
            match opcode {
                0x8 => return Err("firefox closed the connection".to_string()),
                0x9 => self.send_frame(0xA, &payload, deadline)?,
                0xA => {}
                _ => {
                    message.extend_from_slice(&payload);
                    if fin {
                        return String::from_utf8(message).map_err(|_| {
                            "firefox bidi sent a message that is not UTF-8".to_string()
                        });
                    }
                }
            }
        }
    }

    fn read_exact(&mut self, buf: &mut [u8]) -> Result<(), String> {
        self.reader.read_exact(buf).map_err(|why| match why.kind() {
            std::io::ErrorKind::WouldBlock | std::io::ErrorKind::TimedOut => {
                "timed out".to_string()
            }
            _ => format!("firefox bidi: {why}"),
        })
    }
}

/// `Sec-WebSocket-Accept` for `key` (RFC 6455 §4.2.2).
fn accept_for(key: &str) -> String {
    use sha1::{Digest, Sha1};
    let mut hash = Sha1::new();
    hash.update(key.as_bytes());
    hash.update(b"258EAFA5-E914-47DA-95CA-C5AB0DC85B11");
    base64(&hash.finalize())
}

/// Sixteen bytes that differ per call: the handshake key and the frame masks. They need to be unpredictable
/// to a proxy in the path, and the path is a loopback socket, so the std hasher's random seed is plenty.
fn nonce16() -> [u8; 16] {
    use std::hash::{BuildHasher, Hasher};
    let state = std::collections::hash_map::RandomState::new();
    let mut out = [0u8; 16];
    for (i, chunk) in out.chunks_mut(8).enumerate() {
        let mut hasher = state.build_hasher();
        hasher.write_usize(i);
        hasher.write_u128(
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap_or_default()
                .as_nanos(),
        );
        chunk.copy_from_slice(&hasher.finish().to_be_bytes());
    }
    out
}

/// Standard base64 with padding.
fn base64(bytes: &[u8]) -> String {
    const ABC: &[u8; 64] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
    let mut out = String::with_capacity(bytes.len().div_ceil(3) * 4);
    for chunk in bytes.chunks(3) {
        let n = chunk.iter().fold(0u32, |n, b| (n << 8) | *b as u32) << (8 * (3 - chunk.len()));
        for i in 0..4 {
            if i <= chunk.len() {
                out.push(ABC[(n >> (18 - 6 * i)) as usize & 63] as char);
            } else {
                out.push('=');
            }
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_accept_key_is_rfc_6455s_own_example() {
        // RFC 6455 §1.3's worked example.
        assert_eq!(
            accept_for("dGhlIHNhbXBsZSBub25jZQ=="),
            "s3pPLMBiTxaQ9kYGzzhZRbK+xOo="
        );
        assert_eq!(base64(b"f"), "Zg==");
        assert_eq!(base64(b"fo"), "Zm8=");
        assert_eq!(base64(b"foo"), "Zm9v");
    }
}
