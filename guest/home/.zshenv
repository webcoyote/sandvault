# Go programs (gh, and any Go binary) verify TLS through macOS trustd's
# per-user agent, com.apple.trustd.agent. Sandbox processes look up mach
# services in the host user's login session, so allowing that name would
# connect the sandbox to the host user's trustd agent; the profile denies it
# and Go fails with "x509: OSStatus -26276". Point Go (and other tools that
# honor SSL_CERT_FILE) at the system CA bundle instead. Cost: roots added via
# Keychain Access or MDM, and trust-settings overrides, are not honored.
# User configuration below can override this.
if [[ -z "${SSL_CERT_FILE:-}" && -r /etc/ssl/cert.pem ]]; then
    export SSL_CERT_FILE=/etc/ssl/cert.pem
fi

# Load user configuration
if [[ -n "${SHARED_WORKSPACE:-}" && -f "$SHARED_WORKSPACE/user/.zshenv" ]]; then
    source "$SHARED_WORKSPACE/user/.zshenv"
fi
