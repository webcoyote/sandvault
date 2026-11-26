# Probes of the sandbox profile: what a session must be able to do, and what
# it must not. Run inside a sandbox, as text:
#
#   /bin/bash -c "$(cat profile-probes.sh)"
#
# with HOME, TMPDIR and SV2_WORKSPACE set as a session has them. It is run
# two ways: in a real session by tests/integration, and by a unit test of the
# helper (profile_probes) under sandbox-exec as the current user, which needs
# no root and is how the profile is worked on.
#
# Every allow rule in crates/helper/src/base.sb names the probe here that
# fails without it. A rule no probe needs is removed (review-guidelines §3).
#
# One line per probe:
#   ok NAME          as wanted
#   FAIL NAME: why   not as wanted
#   skip NAME: why   the tool is not on this Mac
#
# Some probes use the network: reaching it, and trusting what answers, is
# what they test.
# shellcheck shell=bash
# shellcheck disable=SC2016 # each probe is text for the shell that runs it
WS="${SV2_WORKSPACE:?}"
T="$(/usr/bin/mktemp -d "${TMPDIR:-/tmp}/sv2-probe.XXXXXX")" || { echo "FAIL mktemp: cannot make a scratch directory in ${TMPDIR:-/tmp}"; exit 0; }
PATH="/opt/homebrew/bin:/usr/local/bin:/usr/bin:/bin:/usr/sbin:/sbin"; export PATH
# allowed NAME CMD...: CMD must succeed.
# No probe runs longer than this: a denied service can make a tool wait
# rather than fail. (alarm survives exec.)
BOUNDED=(/usr/bin/perl -e 'alarm shift; exec @ARGV' 40)
allowed() { local name="$1" out; shift; if out="$("${BOUNDED[@]}" "$@" 2>&1)"; then echo "ok $name"; else echo "FAIL $name: $(printf '%s' "$out" | /usr/bin/tr '\n' ' ' | /usr/bin/cut -c1-200)"; fi; }
# denied NAME CMD...: CMD must fail.
denied() { local name="$1" out; shift; if out="$("${BOUNDED[@]}" "$@" 2>&1)"; then echo "FAIL $name: it worked: $(printf '%s' "$out" | /usr/bin/tr '\n' ' ' | /usr/bin/cut -c1-120)"; else echo "ok $name"; fi; }
have() { command -v "$1" >/dev/null 2>&1 || { echo "skip $2: no $1"; return 1; }; }

# --- files
allowed "write in the home" /bin/sh -c 'echo x > "$HOME/.sv2-probe" && rm "$HOME/.sv2-probe"'
allowed "write in the workspace" /bin/sh -c "echo x > '$WS/.sv2-probe' && rm '$WS/.sv2-probe'"
allowed "write in TMPDIR" /bin/sh -c "echo x > '$T/f'"
allowed "write in /tmp" /bin/sh -c 'f=/tmp/sv2-probe.$$; echo x > $f && rm $f'
allowed "read system files" /bin/sh -c 'cat /etc/hosts /usr/share/zoneinfo/UTC >/dev/null && ls /System/Library >/dev/null'
allowed "/dev/null, /dev/urandom, /dev/tty names" /bin/sh -c 'echo x > /dev/null && head -c 8 /dev/urandom >/dev/null'
denied "write outside (/usr/local, /etc, /Library)" /bin/sh -c 'touch /usr/local/sv2-probe || touch /etc/sv2-probe || touch /Library/sv2-probe'
denied "read inside another home" /bin/sh -c 'for d in /Users/*/; do case "$d" in "$HOME/"|/Users/Shared/) continue;; esac; ls "$d" 2>/dev/null | grep -q . && exit 0; done; exit 1'
denied "read /Library/Keychains" /bin/sh -c 'ls /Library/Keychains | grep -q login'
denied "read /Volumes" /bin/sh -c 'ls /Volumes/* 2>/dev/null | grep -q .'
denied "read a raw disk" /bin/sh -c 'head -c 1 /dev/rdisk0'

# --- processes and identity
allowed "fork and exec, pipes" /bin/sh -c 'echo a | tr a b | grep -q b'
allowed "id shows names" /bin/sh -c 'test "$(id -un)" != "$(id -u)" && id -gn >/dev/null'
allowed "ps shows this process" /bin/sh -c '/bin/ps -o pid= -p $$ | grep -q .'
allowed "signals to own processes" /bin/sh -c 'sleep 30 & p=$!; kill $p && ! wait $p'
allowed "sysctl reads" /bin/sh -c 'sysctl -n hw.ncpu kern.ostype >/dev/null'
allowed "date, hostname, uname" /bin/sh -c 'date >/dev/null && uname -a >/dev/null'
allowed "zsh login shell" /bin/zsh -lc 'print -r -- ok'
allowed "open a pty" /usr/bin/python3 -c 'import pty, os; m, s = pty.openpty(); os.write(s, b"x"); os.ttyname(s)'
allowed "shared memory and semaphores (python)" /usr/bin/python3 -c 'import multiprocessing as m; l=m.Lock(); q=m.Queue(); q.put(1); assert q.get()==1'
allowed "mktemp, mkfifo, sockets in TMPDIR" /usr/bin/python3 -c "import socket,os; s=socket.socket(socket.AF_UNIX); s.bind('$T/s'); os.mkfifo('$T/fifo')"

# --- network
allowed "DNS" /usr/bin/python3 -c 'import socket; socket.getaddrinfo("example.com", 443)'
allowed "https with curl (TLS trust)" /usr/bin/curl -fsS -o /dev/null --max-time 20 https://example.com
allowed "https with python (TLS trust)" /usr/bin/python3 -c 'import urllib.request; urllib.request.urlopen("https://example.com", timeout=20).read(1)'
allowed "listen on localhost" /usr/bin/python3 -c 'import socket; s=socket.socket(); s.bind(("127.0.0.1",0)); s.listen(1)'

# --- developer tools
allowed "git init and commit" /bin/sh -c "cd '$T' && git init -q r && cd r && git -c user.name=p -c user.email=p@p commit -q --allow-empty -m x"
allowed "git over https" /bin/sh -c "cd '$T' && GIT_TERMINAL_PROMPT=0 git ls-remote -q https://github.com/octocat/Hello-World.git HEAD | grep -q HEAD"
allowed "ssh-keygen" /usr/bin/ssh-keygen -q -t ed25519 -N '' -f "$T/key"
allowed "compile and run C" /bin/sh -c "cd '$T' && printf 'int main(void){return 0;}' > a.c && cc a.c -o a && ./a"
allowed "python" /usr/bin/python3 -c 'import json, ssl, sqlite3, hashlib; sqlite3.connect(":memory:").execute("select 1")'
# Preferences go through the account's own cfprefsd, which a probe run
# outside a session (as another user, with a scratch HOME) does not have.
[[ -n "${SV2_PROBE_NO_ACCOUNT:-}" ]] || allowed "defaults write and read" /bin/sh -c 'defaults write com.sandvault2.probe k v && test "$(defaults read com.sandvault2.probe k)" = v && defaults delete com.sandvault2.probe'
allowed "keychain create and use" /bin/sh -c "security create-keychain -p '' '$T/k.keychain-db' && security add-generic-password -a a -s s -w w '$T/k.keychain-db' && security find-generic-password -a a -s s -w '$T/k.keychain-db' | grep -q w"
if have node "node"; then
allowed "node runs (JIT) and resolves" node -e 'require("dns").lookup("example.com", (e)=>{ if(e) throw e })'
allowed "node https" node -e 'require("https").get("https://example.com", r=>{ r.resume(); r.on("end", ()=>process.exit(r.statusCode<400?0:1)) }).on("error", e=>{console.error(e); process.exit(1)})'
fi
if have fswatch "fswatch (FSEvents)"; then
allowed "file watcher sees a change (FSEvents)" /bin/sh -c "mkdir '$T/w' && fswatch -1 '$T/w' > '$T/w.out' 2>/dev/null & p=\$!; sleep 1; echo x > '$T/w/f'; sleep 2; kill \$p 2>/dev/null; test -s '$T/w.out'"
fi
if have brew "brew"; then allowed "brew answers" /bin/sh -c 'brew --prefix >/dev/null && brew list --formula >/dev/null'; fi
if have tmux "tmux"; then allowed "tmux server" /bin/sh -c "tmux -S '$T/tmux' new-session -d 'sleep 5' && tmux -S '$T/tmux' kill-server"; fi
if have claude "claude"; then allowed "claude starts" claude --version; fi
# gh is a Go program, and Go verifies certificates through the per-user
# trustd agent. A token that is no token gets the request sent: the answer
# must be GitHub's refusal, which only arrives over a verified connection.
if have gh "gh (Go TLS)"; then allowed "Go program verifies TLS (gh)" /bin/sh -c 'out="$(GH_TOKEN=sv2-probe-not-a-token gh api -X GET https://api.github.com/zen 2>&1)"; echo "$out"; echo "$out" | grep -q "401\|Bad credentials"'; fi
if have xcrun "xcrun"; then allowed "xcrun finds tools" /bin/sh -c 'xcrun --find clang >/dev/null'; fi

# --- a browser, headless, as Playwright starts it. Playwright keeps its
# browsers in the home of whoever installed them. The exit status is not
# judged: only whether the page arrived.
PLAYWRIGHT=""
for candidate in "$HOME"/Library/Caches/ms-playwright/chromium_headless_shell-*/chrome-headless-shell-*/chrome-headless-shell; do
    if [[ -f "$candidate" && -x "$candidate" ]]; then
        allowed "Playwright's headless Chromium loads a page" /bin/sh -c "'$candidate' --headless --no-sandbox --user-data-dir='$T/browser' --dump-dom https://example.com 2>/dev/null | grep -q 'Example Domain'"
        PLAYWRIGHT=found
        break
    fi
done
[[ -n "$PLAYWRIGHT" ]] || echo "skip Playwright's headless Chromium loads a page: none installed in this home"

# --- reaching out of the sandbox
denied "pasteboard" /bin/sh -c 'echo sv2-probe | pbcopy && test "$(pbpaste)" = sv2-probe'
denied "AppleScript to Finder" /usr/bin/osascript -e 'tell application "Finder" to get name of startup disk'
denied "open an application" /bin/sh -c '/usr/bin/open -a Calculator && sleep 1 && pgrep -x Calculator >/dev/null'
denied "diskutil list via DiskArbitration" /bin/sh -c 'diskutil list | grep -q disk0'
denied "launchctl submit" /bin/launchctl submit -l com.sandvault2.probe -- /usr/bin/true
denied "screen capture" /usr/sbin/screencapture -x "$T/shot.png"
/bin/rm -rf "$T" 2>/dev/null
