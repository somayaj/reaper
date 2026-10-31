# Shared macOS Developer ID signing defaults for Reaper release builds.
# shellcheck shell=bash

_reaper_load_macos_signing_env() {
  local root="$1"
  local env_file="$root/packaging/macos/signing.env"
  if [[ -f "$env_file" ]]; then
    # shellcheck disable=SC1090
    set -a
    source "$env_file"
    set +a
  fi
}

_reaper_detect_developer_id_identity() {
  security find-identity -v -p codesigning 2>/dev/null \
    | sed -n 's/.*"\(Developer ID Application:.*\)"/\1/p' \
    | head -1
}

# Export REAPER_SIGN_IDENTITY when unset: signing.env, then Keychain Developer ID.
reaper_ensure_macos_sign_identity() {
  local root="$1"
  _reaper_load_macos_signing_env "$root"

  if [[ -z "${REAPER_SIGN_IDENTITY:-}" ]]; then
    local detected
    detected="$(_reaper_detect_developer_id_identity)"
    if [[ -n "$detected" ]]; then
      export REAPER_SIGN_IDENTITY="$detected"
    fi
  fi

  if [[ -n "${REAPER_SIGN_IDENTITY:-}" && "$REAPER_SIGN_IDENTITY" != "-" ]]; then
    echo "Using codesign identity: $REAPER_SIGN_IDENTITY"
    export REAPER_SIGN_IDENTITY
  else
    echo "No Developer ID identity (ad-hoc sign). Add packaging/macos/signing.env or install cert in Keychain." >&2
  fi
}
