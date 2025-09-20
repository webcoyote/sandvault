The `user` folder is where you can store files that will be copied into the sandvault home directory. It is included in `.gitignore` so they won't be considered part of this repository.

Any zsh configuration files in `user` will be sourced as they are normally:

    .zshenv → .zprofile → .zshrc → .zlogin → .zlogout

All files will be copied to the `$HOME` directory during setup.

Run `sv build --rebuild` after making changes to files in this directory or its children.
