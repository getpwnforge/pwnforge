// crates/api/src/middleware/client_ip.rs
use crate::state::AppState;
use axum::extract::{ConnectInfo, FromRequestParts};
use axum::http::request::Parts;
use std::convert::Infallible;
use std::net::{IpAddr, SocketAddr};

/// The client's IP address, resolved according to `trusted_proxy_count`.
///
/// With `trusted_proxy_count == 0`, the TCP peer address is used directly and
/// the X-Forwarded-For header is ignored entirely: without a trusted proxy in
/// front of the backend, a client could set that header to spoof the IP
/// stored against their own session.
///
/// With `trusted_proxy_count > 0`, the header is trusted up to that many
/// hops: each proxy in the chain appends the peer it received the request
/// from, so the real client address is the entry `trusted_proxy_count`
/// positions from the right, not the left (the leftmost entries are
/// whatever the original client, or an attacker, chose to prepend).
pub struct ClientIp(pub Option<IpAddr>);

impl FromRequestParts<AppState> for ClientIp {
    type Rejection = Infallible;

    async fn from_request_parts(
        parts: &mut Parts,
        state: &AppState,
    ) -> Result<Self, Self::Rejection> {
        let remote = parts
            .extensions
            .get::<ConnectInfo<SocketAddr>>()
            .map(|ConnectInfo(addr)| addr.ip());

        let ip = match remote {
            Some(remote) => {
                let forwarded_for = parts
                    .headers
                    .get("x-forwarded-for")
                    .and_then(|value| value.to_str().ok());
                resolve(forwarded_for, remote, state.config.trusted_proxy_count)
            }
            None => None,
        };

        Ok(ClientIp(ip))
    }
}

/// Pure resolution logic, kept separate from the extractor so it can be
/// tested without spinning up an HTTP request.
///
/// A missing or unresolvable IP returns `None`. It never falls back to
/// `remote` when `trusted > 0` and resolution fails: a missing IP is
/// honest, a forged one is not.
fn resolve(forwarded_for: Option<&str>, remote: IpAddr, trusted: usize) -> Option<IpAddr> {
    if trusted == 0 {
        return Some(remote);
    }

    let header = forwarded_for?;
    if header.trim().is_empty() {
        return None;
    }

    let chain: Vec<&str> = header.split(',').map(|entry| entry.trim()).collect();

    if chain.len() < trusted {
        tracing::warn!(
            chain_len = chain.len(),
            trusted,
            "X-Forwarded-For chain shorter than trusted_proxy_count, dropping client IP"
        );
        return None;
    }

    let client_entry = chain[chain.len() - trusted];
    client_entry.parse::<IpAddr>().ok()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn ip(s: &str) -> IpAddr {
        s.parse().unwrap()
    }

    #[test]
    fn trusted_zero_ignores_header_and_returns_remote() {
        let remote = ip("198.51.100.7");
        assert_eq!(resolve(Some("203.0.113.42"), remote, 0), Some(remote));
    }

    #[test]
    fn trusted_one_single_entry_chain() {
        let remote = ip("198.51.100.7");
        assert_eq!(
            resolve(Some("203.0.113.42"), remote, 1),
            Some(ip("203.0.113.42"))
        );
    }

    #[test]
    fn trusted_one_ignores_client_prepended_entry() {
        let remote = ip("198.51.100.7");
        assert_eq!(
            resolve(Some("1.2.3.4, 203.0.113.42"), remote, 1),
            Some(ip("203.0.113.42"))
        );
    }

    #[test]
    fn trusted_two_picks_second_from_right() {
        let remote = ip("198.51.100.7");
        assert_eq!(
            resolve(Some("1.2.3.4, 203.0.113.42, 10.0.0.1"), remote, 2),
            Some(ip("203.0.113.42"))
        );
    }

    #[test]
    fn trusted_two_chain_too_short_returns_none() {
        let remote = ip("198.51.100.7");
        assert_eq!(resolve(Some("203.0.113.42"), remote, 2), None);
    }

    #[test]
    fn trusted_one_header_absent_returns_none() {
        let remote = ip("198.51.100.7");
        assert_eq!(resolve(None, remote, 1), None);
    }

    #[test]
    fn malformed_entry_returns_none() {
        let remote = ip("198.51.100.7");
        assert_eq!(resolve(Some("not-an-ip"), remote, 1), None);
    }
}
