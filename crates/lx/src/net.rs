/*!
HTTP for catalogs, imports, registries' archives, and media (packages themselves come through
git): one agent for every download, with timeouts, lx's User-Agent, the proxy the environment
names (`HTTPS_PROXY`, `ALL_PROXY`, `NO_PROXY`), and a few tries when a server or the connection
fails. Each download is said on stderr, unless lx prints JSON
*/

use std::{
    io::Read,
    sync::{
        OnceLock,
        atomic::{AtomicBool, Ordering},
    },
    time::Duration,
};

use logoxide::{LogoxideError, Storage, git::GitCli};
use sha2::{Digest, Sha256};

/// How many times a download is tried
const ATTEMPTS: u32 = 3;
/// The largest catalog or import read into memory
const LIMIT: u64 = 1 << 30;

/// Whether downloads go unsaid (JSON output)
static QUIET: AtomicBool = AtomicBool::new(false);

/// Stops saying each download on stderr (for JSON output)
pub fn set_quiet(quiet: bool) {
    QUIET.store(quiet, Ordering::Relaxed);
}

fn agent() -> &'static ureq::Agent {
    static AGENT: OnceLock<ureq::Agent> = OnceLock::new();
    AGENT.get_or_init(|| {
        ureq::Agent::config_builder()
            .user_agent(format!(
                "lx/{} (+https://github.com/Logoxide/logoxide)",
                env!("CARGO_PKG_VERSION")
            ))
            .proxy(ureq::Proxy::try_from_env())
            .timeout_connect(Some(Duration::from_secs(20)))
            .timeout_recv_response(Some(Duration::from_secs(60)))
            // Media can be large: an hour for a body, however slow the line
            .timeout_recv_body(Some(Duration::from_secs(60 * 60)))
            .build()
            .into()
    })
}

/// Whether trying again could help: a server's error, a timeout, or a dropped connection
fn retryable(error: &ureq::Error) -> bool {
    match error {
        ureq::Error::StatusCode(status) => *status >= 500 || *status == 429,
        ureq::Error::Io(_) | ureq::Error::Timeout(_) | ureq::Error::ConnectionFailed => true,
        _ => false,
    }
}

/// `url` fetched with `read`, tried again (after a pause, longer each time) when that could help
fn with_retries<T>(
    url: &str,
    read: impl Fn(ureq::http::Response<ureq::Body>) -> Result<T, ureq::Error>,
) -> logoxide::Result<T> {
    if !QUIET.load(Ordering::Relaxed) {
        eprintln!("Downloading {url}");
    }
    let mut attempt = 1;
    loop {
        match agent().get(url).call().and_then(&read) {
            Ok(value) => return Ok(value),
            Err(e) if attempt < ATTEMPTS && retryable(&e) => {
                std::thread::sleep(Duration::from_millis(500 * 3u64.pow(attempt - 1)));
                attempt += 1;
            }
            Err(e) => {
                return Err(LogoxideError::Fetch {
                    url: url.to_string(),
                    message: e.to_string(),
                });
            }
        }
    }
}

/// A URL's body (catalogs and imports; packages come through git)
pub fn get(url: &str) -> logoxide::Result<Vec<u8>> {
    with_retries(url, |mut response| {
        response.body_mut().with_config().limit(LIMIT).read_to_vec()
    })
}

/**
Downloads a media file and checks it's the one listed (its size, then its SHA-256), hashing as it
reads rather than holding it; errors are named by `url`
*/
pub fn verify_media(url: &str, file: &logoxide::media::MediaFile) -> logoxide::Result<()> {
    let fail = |message: String| LogoxideError::Fetch {
        url: url.to_string(),
        message,
    };
    let (size, hash) = with_retries(url, |response| {
        let mut reader = response.into_body().into_reader();
        let (mut hasher, mut size, mut buffer) = (Sha256::new(), 0u64, vec![0; 1 << 16]);
        loop {
            let n = reader.read(&mut buffer)?;
            if n == 0 {
                break;
            }
            size += n as u64;
            // Already longer than listed: no need to read the rest
            if size > file.size {
                break;
            }
            hasher.update(&buffer[..n]);
        }
        Ok((size, hasher.finalize()))
    })?;
    if size > file.size {
        return Err(fail(format!("more than the {} bytes listed", file.size)));
    }
    if size != file.size {
        return Err(fail(format!("{size} bytes, not the {} listed", file.size)));
    }
    let actual: String = hash.iter().map(|b| format!("{b:02x}")).collect();
    if !actual.eq_ignore_ascii_case(&file.sha256) {
        return Err(fail(format!(
            "its SHA-256 is {actual}, not the {} listed",
            file.sha256
        )));
    }
    Ok(())
}

/// Git through the `git` command, and downloads (registries' catalogs and archives) over HTTPS
pub(crate) struct Web(pub(crate) GitCli);

impl std::ops::Deref for Web {
    type Target = GitCli;
    fn deref(&self) -> &GitCli {
        &self.0
    }
}

impl logoxide::git::Fetcher for Web {
    fn tags(&self, url: &str) -> logoxide::Result<Vec<(String, String)>> {
        self.0.tags(url)
    }

    fn fetch(
        &self,
        url: &str,
        reference: &logoxide::git::GitRef,
        dest: &dyn Storage,
    ) -> logoxide::Result<String> {
        self.0.fetch(url, reference, dest)
    }

    fn fetch_files(
        &self,
        url: &str,
        reference: &logoxide::git::GitRef,
        paths: &[&str],
        dest: &dyn Storage,
    ) -> logoxide::Result<String> {
        self.0.fetch_files(url, reference, paths, dest)
    }

    fn download(&self, url: &str) -> logoxide::Result<Vec<u8>> {
        match url.starts_with("https://") || url.starts_with("http://") {
            true => get(url),
            false => logoxide::git::read_local(url),
        }
    }
}
