export PROMPT="%n@%m %~ %# "

# Use GNU CLI binaries over outdated OSX CLI binaries
if command -v brew &>/dev/null ; then
    BREW_PREFIX="$(brew --prefix)"
    if [[ -d "$BREW_PREFIX/opt/coreutils/libexec/gnubin" ]]; then
        export PATH="$BREW_PREFIX/opt/coreutils/libexec/gnubin:$PATH"
    fi
    if [[ -d "$BREW_PREFIX/opt/gnu-getopt/bin" ]]; then
        export PATH="$BREW_PREFIX/opt/gnu-getopt/bin:$PATH"
    fi
    if [[ -d "$BREW_PREFIX/opt/python/libexec/bin" ]]; then
        export PATH="$BREW_PREFIX/opt/python/libexec/bin:$PATH"
    fi
fi

# My path has high priority than all others
export PATH="$HOME/bin:$PATH"

autoload -Uz +X compinit && compinit

# Case insensitive tab completion
zstyle ':completion:*' matcher-list 'm:{a-zA-Z}={A-Za-z}' 'r:|[._-]=* r:|=*' 'l:|=* r:|=*'

# auto-fill the first viable candidate for tab completion
setopt menucomplete

# vi-editing on command line and for files
bindkey -v
export EDITOR=vi

# Fix zsh bug where tab completion hangs on git commands
# https://superuser.com/a/459057
__git_files () {
    _wanted files expl 'local files' _files
}

# Only allow unique entries in path
typeset -U path

# utilities
command -v bat &>/dev/null && alias cat='bat --paging=never'
command -v direnv &>/dev/null && eval "$(direnv hook zsh)"

# ls
if command -v eza &>/dev/null ; then
    alias ls=eza
    alias l='ls -l --git'
    alias li='ls -l --git --git-ignore'
    alias ll='ls -al --git'
    alias lli='ls -al --git --git-ignore'
    alias tree='ls -lT --git'
else
    alias l='ls -l'
    alias ll='ls -al'
fi


###############################################################################
# Configure sandbox
###############################################################################
"$HOME/configure.sh" || true


###############################################################################
# Run the application
###############################################################################
if [[ -n "${INITIAL_DIR:-}" ]]; then
    cd "$INITIAL_DIR"
fi
if [[ ! -r "$PWD" ]] && [[ -n "${SHARED_WORKSPACE:-}" ]]; then
    cd "${SHARED_WORKSPACE:-}"
fi
if [[ ! -r "$PWD" ]]; then
    cd "$HOME"
fi
if [[ "${COMMAND:-}" != "" ]]; then
    exec "$COMMAND"
fi
