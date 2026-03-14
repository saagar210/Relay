//! Protocol versioning and compatibility rules.
//!
//! Policy: N-1 minor compatibility within the same major version.
//! Example with local 1.2.0:
//! - Compatible: 1.1.x, 1.2.x
//! - Incompatible: 1.0.x, 2.x

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct ParsedVersion {
    major: u16,
    minor: u16,
    patch: u16,
}

/// Current wire protocol version used by Relay peers/signaling metadata.
pub const CURRENT_PROTOCOL_VERSION: &str = "1.1.0";

/// Human-readable policy marker for docs/telemetry.
pub const COMPATIBILITY_POLICY: &str = "N-1 minor (same major)";

fn parse_version(value: &str) -> Option<ParsedVersion> {
    let mut parts = value.split('.');
    let major = parts.next()?.parse::<u16>().ok()?;
    let minor = parts.next()?.parse::<u16>().ok()?;
    let patch = parts.next()?.parse::<u16>().ok()?;

    // Strict semver core-only format: exactly 3 numeric segments.
    if parts.next().is_some() {
        return None;
    }

    Some(ParsedVersion {
        major,
        minor,
        patch,
    })
}

/// Returns true when `peer_version` is compatible with local protocol.
///
/// Compatibility rule:
/// - Same major required.
/// - Peer minor must be in [local_minor - 1, local_minor].
pub fn is_peer_protocol_compatible(peer_version: &str) -> bool {
    let local = match parse_version(CURRENT_PROTOCOL_VERSION) {
        Some(v) => v,
        None => return false,
    };
    let peer = match parse_version(peer_version) {
        Some(v) => v,
        None => return false,
    };

    if peer.major != local.major {
        return false;
    }

    let min_minor = local.minor.saturating_sub(1);
    peer.minor >= min_minor && peer.minor <= local.minor
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn accepts_same_minor() {
        assert!(is_peer_protocol_compatible("1.1.0"));
        assert!(is_peer_protocol_compatible("1.1.9"));
    }

    #[test]
    fn accepts_n_minus_one_minor() {
        assert!(is_peer_protocol_compatible("1.0.0"));
        assert!(is_peer_protocol_compatible("1.0.99"));
    }

    #[test]
    fn rejects_older_than_n_minus_one() {
        assert!(!is_peer_protocol_compatible("0.9.0"));
    }

    #[test]
    fn rejects_newer_minor_or_major() {
        assert!(!is_peer_protocol_compatible("1.2.0"));
        assert!(!is_peer_protocol_compatible("2.0.0"));
    }

    #[test]
    fn rejects_invalid_format() {
        assert!(!is_peer_protocol_compatible("1.1"));
        assert!(!is_peer_protocol_compatible("v1.1.0"));
        assert!(!is_peer_protocol_compatible("foo"));
    }
}
