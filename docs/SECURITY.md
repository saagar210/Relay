# Security Model

## Overview

Relay uses end-to-end encryption with no trusted third party. The signaling server is untrusted and cannot decrypt file contents.

## Cryptography

### SPAKE2 Key Exchange

**Purpose**: Derive shared encryption key from transfer code (password)

- **Algorithm**: SPAKE2 (Ed25519 group)
- **Password**: Transfer code (e.g., `42-elephant-zebra`)
- **Output**: 32-byte shared secret
- **Protection**: Prevents MITM attacks during key derivation
- **Implementation**: `spake2` crate (v0.4)

**Security Properties**:
- Both parties must know the code
- Server cannot derive the key (even if it sees messages)
- Resistant to offline dictionary attacks

### AES-256-GCM Encryption

**Purpose**: Encrypt file chunks during transfer

- **Algorithm**: AES-256-GCM
- **Key**: Derived from SPAKE2 exchange
- **Nonce**: 4-byte random prefix + 8-byte counter
- **Chunk Size**: 256 KB
- **Implementation**: `ring` crate (v0.17)

**Security Properties**:
- Authenticated encryption (detects tampering)
- Unique nonce per chunk (prevents replay)
- Counter prevents nonce reuse

### SHA-256 File Integrity

**Purpose**: Verify files after transfer

- **Algorithm**: SHA-256
- **Timing**: Computed during transfer, verified after last chunk
- **Implementation**: `sha2` crate (v0.10)

**Security Properties**:
- Cryptographic hash (collision-resistant)
- Detects corruption or tampering
- Independent of encryption (defense in depth)

## Threat Model

### In Scope

✓ **Passive network eavesdropper**: Cannot decrypt file contents (AES-256-GCM)
✓ **Active MITM attacker**: Blocked by SPAKE2 key exchange
✓ **Malicious signaling server**: Sees only encrypted payloads
✓ **Path traversal attacks**: Sanitized (blocks `..`, absolute paths, null bytes)
✓ **Data tampering**: Detected by GCM auth tag + SHA-256

### Out of Scope

✗ **Transfer code interception**: If attacker obtains code, they can decrypt (physical security required)
✗ **Endpoint compromise**: If sender/receiver machine is compromised, files are exposed
✗ **Traffic analysis**: Signaling server observes IPs, timing, chunk sizes (but not content)
✗ **DoS attacks**: Rate limiting helps, but determined attacker can exhaust relay bandwidth

## Input Validation

### Path Sanitization (`protocol/reassembler.rs`)

```rust
fn sanitize_path(p: &str) -> Result<()> {
  if p.contains("..") { return Err(PathTraversal) }
  if Path::new(p).is_absolute() { return Err(PathTraversal) }
  if p.contains('\0') { return Err(PathTraversal) }
  Ok(())
}
```

**Tests**: 9 test cases covering edge cases (multiple `..`, root paths, null bytes)

### Transfer Code Validation (`transfer/code.rs`)

- **Format**: `digit-word-word` (e.g., `42-elephant-zebra`)
- **Validation**: Regex check, wordlist verification
- **Entropy**: 8 bits (digit) + 8 bits (word 1) + 8 bits (word 2) = ~16.8M combinations

### Message Length Limits

- **Max chunk size**: 256 KB (configurable)
- **Max message size**: 16 MB (prevents memory exhaustion)
- **Max file metadata**: Reasonable name/path lengths

## Known Limitations

1. **Transfer code strength**: ~16.8M combinations (8+8+8 bits from 256-word list)
   - **Mitigation**: Codes expire after 10 minutes (session TTL)
   - **Risk**: Brute force requires real-time connection attempts

2. **Signaling server observability**:
   - Sees IP addresses (sender + receiver)
   - Sees transfer timing and chunk boundaries
   - **Mitigation**: Content is encrypted, server is untrusted by design

3. **Relay bandwidth**:
   - Default rate limit: 10 MB/s per session
   - **Mitigation**: Prefer direct QUIC when possible

## Vulnerability Gate Policy

- CI vulnerability scanning is hard-fail for:
  - Go (`govulncheck`)
  - Rust (`cargo audit`)
  - JavaScript (`pnpm audit`)
- Release preflight is hard-fail for required supply-chain evidence:
  - SBOM artifact
  - Provenance artifact
  - Artifact signature record

- Rust advisory exceptions are tracked in `client/src-tauri/.cargo/audit.toml`.
  - These are currently upstream-driven GTK3/Tauri transitive advisories.
  - Exceptions are temporary and must be removed as upstream remediations land.

## Vulnerability Disclosure

If you discover a security vulnerability, please report it to:

- **GitHub Issues** (for non-critical issues): https://github.com/your-org/relay/issues
- **Email** (for critical issues): security@example.com

We commit to:
- Acknowledging reports within 48 hours
- Providing fixes within 30 days for critical issues
- Crediting researchers (unless anonymity requested)

## Security Checklist

Before deploying:

- [ ] Use strong signaling server TLS (Let's Encrypt)
- [ ] Review session TTL (default: 10 min)
- [ ] Monitor relay bandwidth usage
- [ ] Verify transfer codes are communicated securely (out-of-band)
- [ ] Keep dependencies updated (run `cargo audit` + `go mod verify`)
- [ ] Enable logging for security events
- [ ] Review server capacity limits (max sessions: 1000 default)
