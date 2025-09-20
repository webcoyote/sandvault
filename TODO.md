- running as ssh or sudo doesn't allow running iOS simulator


brew tap awakecoding/tap
brew install mcp-proxy-tool

brew tap facebook/fb
brew install idb-companion

brew tap facebook/fb
brew install idb-companion
pipx install fb-idb

mcp-proxy-tool -c npx -a ios-simulator-mcp

claude mcp add --scope user ios-simulator-mcp ~/bin/connect.sh ios-simulator-mcp

# listener
```
socat UNIX-LISTEN:shell.sock,group="sandvault-$USER",fork EXEC:"/bin/bash -i",pty,stderr

USR="sandvault-$USER"
FIL="/Users/sandvault-$USER/.mcp.sock"
CMD=
socat UNIX-LISTEN:"$FIL",group="$USR",fork EXEC:"$CMD",pty,stderr
```

# connector
```
socat - UNIX-CONNECT:shell.sock
```
