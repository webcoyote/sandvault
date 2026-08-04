# shellcheck shell=bash
# shellcheck disable=SC2034  # variables consumed via indirection by sourcing scripts
# Agent → (sandbox subdir, mirror link, agentsview TOML key, host default) tuples
# for the agentsview export feature. Single source of truth, sourced by both
# the host-side installer in `sv` and the sandbox-side setup-merge generator.
#
# Use bash 3.2-safe variable indirection: AGENTSVIEW_<field>_<agent>="..."
# rather than associative arrays.

AGENTSVIEW_AGENTS=(claude codex opencode gemini pi)

# Sandbox-side path under /Users/sandvault-$USER/
AGENTSVIEW_SUBDIR_claude=".claude/projects"
AGENTSVIEW_SUBDIR_codex=".codex/sessions"
AGENTSVIEW_SUBDIR_opencode=".local/share/opencode"
AGENTSVIEW_SUBDIR_gemini=".gemini"
AGENTSVIEW_SUBDIR_pi=".pi/agent/sessions"

# Optional: a directory on the way to SUBDIR that holds credentials and must
# not become group-readable. `mkdir -p` creates intermediates at 0755 under the
# default umask, and an agent that would have created the directory itself with
# tighter permissions never gets the chance -- pi's auth-storage applies 0700 at
# mkdir time only, and never chmods a directory that already exists. Only set
# this where the agent keeps secrets in a parent of its session directory; the
# other agents' session dirs have no such parent, so they have no entry.
AGENTSVIEW_PRIVATE_pi=".pi/agent"

# Host-side default path under $HOME (matches agentsview's
# parser.Registry DefaultDirs)
AGENTSVIEW_DEFAULT_claude=".claude/projects"
AGENTSVIEW_DEFAULT_codex=".codex/sessions"
AGENTSVIEW_DEFAULT_opencode=".local/share/opencode"
AGENTSVIEW_DEFAULT_gemini=".gemini"
AGENTSVIEW_DEFAULT_pi=".pi/agent/sessions"

# Top-level TOML key in ~/.agentsview/config.toml
AGENTSVIEW_TOMLKEY_claude="claude_project_dirs"
AGENTSVIEW_TOMLKEY_codex="codex_sessions_dirs"
AGENTSVIEW_TOMLKEY_opencode="opencode_dirs"
AGENTSVIEW_TOMLKEY_gemini="gemini_dirs"
AGENTSVIEW_TOMLKEY_pi="pi_dirs"

# Look up a field for an agent. Usage: agentsview_field SUBDIR claude
agentsview_field() {
    local field="$1"
    local agent="$2"
    local var="AGENTSVIEW_${field}_${agent}"
    eval "printf '%s' \"\${$var:-}\""
}
