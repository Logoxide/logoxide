/*!
HTTP for catalogs, imports, registries' archives, and media (packages themselves come through git,
or [`crate::fetch::NetFetcher`] where there's no `git`). [`Http`] is what everything here
downloads through; a host gives its own (the bindings ask theirs), or takes [`UreqHttp`] (the
`ureq` feature): one agent for every download, with timeouts, a User-Agent, the proxy the
environment names (`HTTPS_PROXY`, `ALL_PROXY`, `NO_PROXY`), and a few tries when a server or the
connection fails. Each download is said through the [`Notes`](crate::Notes) it's given
*/

use std::ops::ControlFlow;

use logoxide::Result;

/// Downloads by URL
pub trait Http: Send + Sync {
    /// A URL's body, whole (catalogs, imports, archives)
    fn get(&self, url: &str) -> Result<Vec<u8>>;

    /**
    A URL's body handed to `sink` as it's read, so a large one (media) is never held whole;
    `sink` gives [`ControlFlow::Break`] to stop reading. A download is tried again only before
    `sink` has had any of it. By default the body is read whole by [`Http::get`], then given
    */
    fn get_to(&self, url: &str, sink: &mut dyn FnMut(&[u8]) -> ControlFlow<()>) -> Result<()> {
        let bytes = self.get(url)?;
        let _ = sink(&bytes);
        Ok(())
    }
}

impl<T: Http + ?Sized> Http for std::sync::Arc<T> {
    fn get(&self, url: &str) -> Result<Vec<u8>> {
        (**self).get(url)
    }

    fn get_to(&self, url: &str, sink: &mut dyn FnMut(&[u8]) -> ControlFlow<()>) -> Result<()> {
        (**self).get_to(url, sink)
    }
}

/// No downloads: every URL is an error (a host that only reads what it has)
#[derive(Clone, Copy, Debug, Default)]
pub struct NoHttp;

impl Http for NoHttp {
    fn get(&self, url: &str) -> Result<Vec<u8>> {
        Err(logoxide::LogoxideError::network(
            url,
            "no way to download here",
        ))
    }
}

#[cfg(feature = "ureq")]
pub use self::ureq_http::UreqHttp;

#[cfg(feature = "ureq")]
mod ureq_http {
    use std::{cell::Cell, io::Read, ops::ControlFlow, time::Duration};

    use logoxide::{LogoxideError, Result, git::is_insecure};
    use ureq::ResponseExt;

    use crate::{Note, Notes};

    /// How many times a download is tried
    const ATTEMPTS: u32 = 3;
    /// The largest catalog or import read into memory
    const LIMIT: u64 = 1 << 30;

    /// [`Http`](super::Http) through ureq
    #[derive(Clone)]
    pub struct UreqHttp {
        agent: ureq::Agent,
        notes: Notes,
    }

    impl UreqHttp {
        /**
        An agent sending `user_agent` (`lx/0.2.0 (+https://github.com/Logoxide/logoxide)`), with
        the environment's proxy, 20 s to connect, 60 s for a response, and an hour for a body
        (media can be large, however slow the line); each download and plain HTTP URL is said
        through `notes`
        */
        pub fn new(user_agent: &str, notes: Notes) -> UreqHttp {
            let agent = ureq::Agent::config_builder()
                .user_agent(user_agent)
                .proxy(ureq::Proxy::try_from_env())
                .timeout_connect(Some(Duration::from_secs(20)))
                .timeout_recv_response(Some(Duration::from_secs(60)))
                .timeout_recv_body(Some(Duration::from_secs(60 * 60)))
                .build()
                .into();
            UreqHttp { agent, notes }
        }

        /**
        `url` fetched with `read`, tried again (after a pause, longer each time) when that could
        help and `retry` allows it
        */
        fn with_retries<T>(
            &self,
            url: &str,
            retry: &dyn Fn() -> bool,
            read: &mut dyn FnMut(ureq::http::Response<ureq::Body>) -> Result<T, ureq::Error>,
        ) -> Result<T> {
            (self.notes)(&Note::Downloading {
                url: url.to_string(),
            });
            let (mut attempt, warned) = (1, Cell::new(false));
            loop {
                let result = self.call(url).and_then(|response| {
                    // Judged by where it ended up: an HTTP URL redirected elsewhere is still said
                    let at = response.get_uri().to_string();
                    if !warned.replace(true) && (is_insecure(url) || is_insecure(&at)) {
                        (self.notes)(&Note::Insecure {
                            url: url.to_string(),
                            redirected_to: (at != url).then_some(at),
                        });
                    }
                    read(response)
                });
                match result {
                    Ok(value) => return Ok(value),
                    Err(e) if attempt < ATTEMPTS && retryable(&e) && retry() => {
                        std::thread::sleep(Duration::from_millis(500 * 3u64.pow(attempt - 1)));
                        attempt += 1;
                    }
                    Err(e) => {
                        return Err(LogoxideError::Network {
                            url: url.to_string(),
                            message: describe(&e),
                            source: Some(Box::new(e)),
                        });
                    }
                }
            }
        }

        /// Asks for `url`. An HTTPS URL stays HTTPS: ureq checks every hop of a redirect, and
        /// one to plain HTTP is refused
        fn call(&self, url: &str) -> Result<ureq::http::Response<ureq::Body>, ureq::Error> {
            let https = url
                .get(..8)
                .is_some_and(|s| s.eq_ignore_ascii_case("https://"));
            (self.agent.get(url).config())
                .https_only(https)
                .build()
                .call()
        }
    }

    impl super::Http for UreqHttp {
        fn get(&self, url: &str) -> Result<Vec<u8>> {
            self.with_retries(url, &|| true, &mut |mut response| {
                response.body_mut().with_config().limit(LIMIT).read_to_vec()
            })
        }

        fn get_to(&self, url: &str, sink: &mut dyn FnMut(&[u8]) -> ControlFlow<()>) -> Result<()> {
            let given = Cell::new(false);
            self.with_retries(url, &|| !given.get(), &mut |response| {
                let mut reader = response.into_body().into_reader();
                let mut buffer = vec![0; 1 << 16];
                loop {
                    let n = reader.read(&mut buffer)?;
                    if n == 0 {
                        return Ok(());
                    }
                    given.set(true);
                    if sink(&buffer[..n]).is_break() {
                        return Ok(());
                    }
                }
            })
        }
    }

    /// Whether trying again could help: a server's error, a timeout, or a dropped connection
    fn retryable(error: &ureq::Error) -> bool {
        match error {
            ureq::Error::StatusCode(status) => *status >= 500 || *status == 429,
            ureq::Error::Io(_) | ureq::Error::Timeout(_) | ureq::Error::ConnectionFailed => true,
            _ => false,
        }
    }

    /// What went wrong, in words
    fn describe(error: &ureq::Error) -> String {
        match error {
            ureq::Error::RequireHttpsOnly(to) => {
                format!("refused: it redirects to {to}, which isn't HTTPS")
            }
            other => other.to_string(),
        }
    }

    #[cfg(test)]
    mod tests {
        use super::*;
        use crate::http::Http;
        use std::io::{BufRead, BufReader, Write};

        /// A server on this machine answering `/from` with a redirect to `/to`, and `/to` with
        /// `ok`
        fn serve() -> String {
            let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
            let base = format!("http://{}", listener.local_addr().unwrap());
            let location = format!("{base}/to");
            std::thread::spawn(move || {
                for stream in listener.incoming().flatten() {
                    let mut reader = BufReader::new(&stream);
                    let mut line = String::new();
                    reader.read_line(&mut line).unwrap();
                    // The headers, to the blank line
                    while reader.read_line(&mut String::new()).unwrap_or(0) > 2 {}
                    let reply = match line.contains("/from") {
                        true => format!(
                            "HTTP/1.1 302 Found\r\nLocation: {location}\r\nContent-Length: 0\r\n\r\n"
                        ),
                        false => String::from("HTTP/1.1 200 OK\r\nContent-Length: 2\r\n\r\nok"),
                    };
                    let _ = (&stream).write_all(reply.as_bytes());
                }
            });
            base
        }

        #[test]
        fn https_never_becomes_http() {
            let base = serve();
            let said = std::sync::Arc::new(std::sync::Mutex::new(vec![]));
            let notes: Notes = {
                let said = said.clone();
                std::sync::Arc::new(move |note: &Note| said.lock().unwrap().push(note.clone()))
            };
            let http = UreqHttp::new("test", notes);
            // Plain HTTP may be redirected (and is warned of)
            assert_eq!(http.get(&format!("{base}/from")).unwrap(), b"ok");
            assert!(said.lock().unwrap().iter().any(|n| matches!(
                n,
                Note::Insecure {
                    redirected_to: Some(_),
                    ..
                }
            )));
            // Streamed the same
            let mut got = vec![];
            (http.get_to(&format!("{base}/to"), &mut |bytes| {
                got.extend_from_slice(bytes);
                ControlFlow::Continue(())
            }))
            .unwrap();
            assert_eq!(got, b"ok");
            // A request that must stay HTTPS (as every HTTPS URL's must) refuses a hop that
            // isn't, and says so
            let error = (http.agent.get(format!("{base}/from")).config())
                .https_only(true)
                .build()
                .call()
                .unwrap_err();
            let message = describe(&error);
            assert!(message.contains("isn't HTTPS"), "{message}");
        }
    }
}
