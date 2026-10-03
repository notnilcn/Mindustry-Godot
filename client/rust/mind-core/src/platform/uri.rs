// Ported from Mindustry (https://github.com/Anuken/Mindustry) — GPL-3.0.
// SPDX-License-Identifier: GPL-3.0-only

//! `mindustry://` / `mindustry-godot://` connect-URI parsing (plan 22 §6.7).
//!
//! Port of `ClientLauncher`'s protocol handling: the dual scheme keeps this
//! build from hijacking a Java Mindustry install on the same machine (P22-9).
//! A URI may carry `host`, `host:port`, or an explicit `connect/`-prefixed
//! authority; the parser only extracts the endpoint (plan 21 owns the join
//! flow).

/// URIs this build registers, most-specific first.
pub const SCHEMES: [&str; 2] = ["mindustry-godot", "mindustry"];

/// Authority prefixes stripped before the host/port pair.
const CONNECT_PREFIXES: [&str; 3] = ["connect", "join", "play"];

/// A parsed connect endpoint from a URI.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ConnectUri {
    /// Scheme that matched (one of [`SCHEMES`]).
    pub scheme: String,
    /// Host name or address.
    pub host: String,
    /// Optional explicit port.
    pub port: Option<u16>,
}

/// Parses `scheme://[connect/]host[:port]`; returns `None` for other schemes or
/// an empty host.
pub fn parse(uri: &str) -> Option<ConnectUri> {
    let (scheme, rest) = uri.split_once("://")?;
    let scheme = scheme.to_ascii_lowercase();
    if !SCHEMES.contains(&scheme.as_str()) {
        return None;
    }

    // Drop query/fragment, then optional `connect/`-style prefix, then the
    // path; what remains is `host[:port]`.
    let authority = rest.split(['?', '#']).next().unwrap_or("");
    let authority = authority.trim_start_matches('/');
    let authority = CONNECT_PREFIXES
        .iter()
        .find_map(|prefix| authority.strip_prefix(&format!("{prefix}/")))
        .unwrap_or(authority);
    let authority = authority.split('/').next().unwrap_or("");
    if authority.is_empty() {
        return None;
    }

    let (host, port) = match authority.rsplit_once(':') {
        Some((host, port)) if !host.is_empty() => {
            let port = port.parse::<u16>().ok()?;
            (host.to_owned(), Some(port))
        }
        Some(_) => return None,
        None => (authority.to_owned(), None),
    };
    if host.is_empty() {
        return None;
    }
    Some(ConnectUri { scheme, host, port })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_host_and_port() {
        let uri = parse("mindustry://127.0.0.1:6567").expect("parse");
        assert_eq!(uri.scheme, "mindustry");
        assert_eq!(uri.host, "127.0.0.1");
        assert_eq!(uri.port, Some(6567));

        let bare = parse("mindustry-godot://example.com").expect("parse");
        assert_eq!(bare.host, "example.com");
        assert_eq!(bare.port, None);
    }

    #[test]
    fn strips_connect_prefix_and_query() {
        let uri = parse("mindustry://connect/example.com:7000?x=1").expect("parse");
        assert_eq!(uri.host, "example.com");
        assert_eq!(uri.port, Some(7000));
    }

    #[test]
    fn rejects_other_schemes_and_bad_authorities() {
        assert!(parse("https://example.com").is_none());
        assert!(parse("mindustry://").is_none());
        assert!(parse("mindustry://:6567").is_none());
        assert!(parse("mindustry://host:notaport").is_none());
    }
}
