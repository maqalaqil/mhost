# Security Policy

## Supported Versions

| Version | Supported |
|---------|-----------|
| 0.1.x   | Yes       |

## Reporting a Vulnerability

If you discover a security vulnerability in mhost, please report it responsibly:

1. **Do not** open a public GitHub issue
2. Email: security@mhost.dev (or create a private security advisory on GitHub)
3. Include:
   - Description of the vulnerability
   - Steps to reproduce
   - Potential impact
   - Suggested fix (if any)

## Response Timeline

- **Acknowledgment**: Within 48 hours
- **Assessment**: Within 1 week
- **Fix**: Depends on severity (critical: ASAP, high: 1 week, medium: 2 weeks)

## Scope

The following are in scope:
- The `mhost` and `mhostd` binaries
- IPC protocol security
- SSH key handling in `mhost cloud`
- Bot token storage in `mhost bot`
- API key handling in `mhost ai`
- Notification webhook security (HMAC signing)

## Known Security Considerations

- API keys and bot tokens are stored in `~/.mhost/*.json` — ensure proper file permissions
- SSH keys used by `mhost cloud` follow system SSH conventions
- The IPC socket at `~/.mhost/mhostd.sock` is accessible to the current user only
