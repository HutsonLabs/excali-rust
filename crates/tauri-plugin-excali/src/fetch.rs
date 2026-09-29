//! The allow-listed library fetch: what `<excali-editor>`'s
//! `library-fetch` event asks the host for.
//!
//! Upstream checks a library URL with `validateLibraryUrl` against
//! `ALLOWED_LIBRARY_URLS` (or the editor's own list) and then fetches it
//! (`packages/excalidraw/data/library.ts:497-528, 726-763`). The editor
//! runs the check before it asks (`excali_core::library_url`); the plugin
//! runs it again on the native side, because the webview's word is not
//! enough for a request made with the app's network access, and holds
//! every redirect to the same list (a browser's `fetch` would follow any).
//! Only `https` URLs are fetched, and a body larger than
//! [`MAX_LIBRARY_BYTES`] is refused.

use std::io::Read;

use excali_core::library_url::{
    validate_library_url_with, LibraryUrlError, LibraryUrlValidator, ALLOWED_LIBRARY_URLS,
};
use url::Url;

use crate::error::{Error, Result};

/// The largest library body read: 64 MiB, twice the size of the largest
/// library in the catalogue's `libraries.json` would need to be for it to
/// matter.
pub const MAX_LIBRARY_BYTES: u64 = 64 * 1024 * 1024;

/// Redirects followed before giving up (a browser's `fetch` stops at 20).
pub const MAX_REDIRECTS: usize = 20;

/// One HTTP response, redirects not followed.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct HttpResponse {
    pub status: u16,
    /// The `Location` header.
    pub location: Option<String>,
    pub body: Vec<u8>,
}

/// One `GET` without following redirects, reading at most `limit` bytes of
/// the body (more is an error).
pub trait Transport: Send + Sync {
    fn get(&self, url: &str, limit: u64) -> std::result::Result<HttpResponse, String>;
}

/// The network, through `ureq` (rustls, no redirects followed).
pub struct UreqTransport {
    agent: ureq::Agent,
}

impl Default for UreqTransport {
    fn default() -> Self {
        let config = ureq::Agent::config_builder()
            .max_redirects(0)
            .http_status_as_error(false)
            .https_only(true)
            .build();
        UreqTransport {
            agent: config.into(),
        }
    }
}

impl Transport for UreqTransport {
    fn get(&self, url: &str, limit: u64) -> std::result::Result<HttpResponse, String> {
        let mut response = self.agent.get(url).call().map_err(|e| e.to_string())?;
        let status = response.status().as_u16();
        let location = response
            .headers()
            .get("location")
            .and_then(|v| v.to_str().ok())
            .map(str::to_owned);
        let mut body = Vec::new();
        response
            .body_mut()
            .as_reader()
            .take(limit + 1)
            .read_to_end(&mut body)
            .map_err(|e| e.to_string())?;
        Ok(HttpResponse {
            status,
            location,
            body,
        })
    }
}

/// The allow-list and the transport.
pub struct LibraryFetcher {
    allow_list: Vec<String>,
    transport: Box<dyn Transport>,
}

impl Default for LibraryFetcher {
    /// Upstream's `ALLOWED_LIBRARY_URLS` over the network.
    fn default() -> Self {
        LibraryFetcher::new(
            ALLOWED_LIBRARY_URLS
                .iter()
                .map(|s| (*s).to_owned())
                .collect(),
            Box::new(UreqTransport::default()),
        )
    }
}

impl LibraryFetcher {
    pub fn new(allow_list: Vec<String>, transport: Box<dyn Transport>) -> LibraryFetcher {
        LibraryFetcher {
            allow_list,
            transport,
        }
    }

    pub fn allow_list(&self) -> &[String] {
        &self.allow_list
    }

    /// `validateLibraryUrl(url, allowList)`, then `https` only.
    pub fn check(&self, url: &str) -> Result<()> {
        let list: Vec<&str> = self.allow_list.iter().map(String::as_str).collect();
        validate_library_url_with(url, &LibraryUrlValidator::AllowList(&list))?;
        let parsed = Url::parse(url).map_err(|_| LibraryUrlError::InvalidUrl)?;
        if parsed.scheme() != "https" {
            return Err(LibraryUrlError::Disallowed {
                url: url.to_owned(),
            }
            .into());
        }
        Ok(())
    }

    /// The library at `url` as text (UTF-8, invalid sequences replaced, as
    /// `blob.text()` reads it): the URL and each redirect checked, the
    /// response a 2xx.
    pub fn fetch(&self, url: &str) -> Result<String> {
        let mut current = url.to_owned();
        for _ in 0..=MAX_REDIRECTS {
            self.check(&current)?;
            let response = self
                .transport
                .get(&current, MAX_LIBRARY_BYTES)
                .map_err(Error::Fetch)?;
            match response.status {
                200..=299 => {
                    if response.body.len() as u64 > MAX_LIBRARY_BYTES {
                        return Err(Error::Fetch(format!(
                            "{current}: larger than {MAX_LIBRARY_BYTES} bytes"
                        )));
                    }
                    return Ok(String::from_utf8_lossy(&response.body).into_owned());
                }
                301 | 302 | 303 | 307 | 308 => {
                    let location = response.location.ok_or_else(|| {
                        Error::Fetch(format!(
                            "{current}: HTTP {} without a Location",
                            response.status
                        ))
                    })?;
                    current = Url::parse(&current)
                        .and_then(|base| base.join(&location))
                        .map_err(|_| LibraryUrlError::InvalidUrl)?
                        .into();
                }
                status => return Err(Error::Fetch(format!("{current}: HTTP {status}"))),
            }
        }
        Err(Error::Fetch(format!(
            "{url}: more than {MAX_REDIRECTS} redirects"
        )))
    }
}
