#!/bin/bash
# Entrypoint wrapper for unleash sandbox containers.
#
# Fixes DNS under gVisor: Docker's internal resolver (127.0.0.11)
# doesn't work with gVisor's network stack. We overwrite resolv.conf
# with real nameservers if the internal resolver is unreachable.
#
# Privilege handling:
#   - Agent CLIs (claude, codex, gemini, unleash, opencode) run as 'unleash' user
#     because Claude Code refuses --dangerously-skip-permissions as root.
#   - bash/shell sessions run as root for full install capability.
# gVisor + network isolation is the security boundary, not the in-container user.

# Fix DNS if Docker's internal resolver is broken (gVisor)
if ! getent hosts google.com >/dev/null 2>&1; then
    cat > /etc/resolv.conf 2>/dev/null <<'EOF'
nameserver 8.8.8.8
nameserver 8.8.4.4
EOF
fi

# If the image is invoked with a flag as the first argument (e.g. `docker run
# image --version`), docker replaces the default CMD and hands us just the flag.
# Bash's `exec` builtin then tries to parse it as its own option and dies with
# "exec: --: invalid option". Treat flag-first invocations as arguments to the
# default `unleash` binary, matching the intent of `CMD ["unleash"]`.
if [[ "${1:-}" == -* ]]; then
    set -- unleash "$@"
fi

# Determine if we should drop privileges
cmd="$(basename "${1:-}" 2>/dev/null)"
case "$cmd" in
    claude|codex|gemini|opencode|unleash)
        # Agent CLIs need non-root (Claude Code refuses --dangerously-skip-permissions as root)
        export HOME=/home/unleash
        exec runuser -u unleash -- "$@"
        ;;
    *)
        # bash/shell/other: stay root for full access
        export HOME=/home/unleash
        exec "$@"
        ;;
esac
