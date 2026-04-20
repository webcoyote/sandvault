#!/bin/bash
# Shared helper for AI agent scripts.
# Assembles tool-awareness prompts based on available environment variables.
#
# Usage:
#   source "$(dirname "${BASH_SOURCE[0]}")/sv-tool-prompts.sh"
#   prompt=$(sv_tool_prompt)
#
# To add a new tool:
#   1. Create a prompt file in prompts/<tool>.md
#   2. Add a mapping entry to the array below (env-var -> file)

# Map: environment variable -> prompt file basename (without .md)
_SV_TOOL_MAP=(
    "SV_BROWSER_ENDPOINT:browser"
    "SV_IOS_SIMULATOR_ENDPOINT:ios-simulator"
)

# Assemble prompt text from all active tools.
# Returns empty string if no tools are active.
sv_tool_prompt() {
    local prompts_dir
    prompts_dir="$(dirname "${BASH_SOURCE[0]}")/prompts"
    local result=""

    for entry in "${_SV_TOOL_MAP[@]}"; do
        local env_var="${entry%%:*}"
        local file="${entry#*:}"

        # Check if the environment variable is set and non-empty
        if [[ -n "${!env_var:-}" ]]; then
            local path="$prompts_dir/${file}.md"
            if [[ -r "$path" ]]; then
                if [[ -n "$result" ]]; then
                    result+=$'\n\n'
                fi
                result+="$(cat "$path")"
            else
                echo >&2 "WARNING: tool prompt file not found: $path"
            fi
        fi
    done

    printf '%s' "$result"
}
