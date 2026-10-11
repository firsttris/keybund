# Installation

Keybund comes as a desktop app (AppImage or `.deb` for Linux, `.dmg` for macOS) and as a
command-line tool (`keybund`). Both work on the same data, so you can use either or both. Every release is on the
[releases page](https://github.com/firsttris/keybund/releases/latest), with a `SHA256SUMS` file.

## Requirements

| | Needed for |
|---|---|
| Linux, x86_64 | built on Ubuntu 22.04, so it runs on that and anything newer (Fedora, Bazzite, Arch, Debian 12, openSUSE …) |
| macOS 11 or newer | Apple Silicon and Intel in one app; uses Apple's own `ssh` and `git` (Xcode Command Line Tools) |
| OpenSSH client (`ssh`, `ssh-keygen`, `ssh-keyscan`) | creating keys, connection tests, host keys – installed almost everywhere |
| `git` | Git folders, “which key?”, cloning |
| an `ssh-agent` | optional, recommended: a key with a passphrase is unlocked once per session (KDE, GNOME and most desktops start one) |
| `ksshaskpass` or another `ssh-askpass` | Linux, optional: lets the app ask for a server password or a passphrase in a dialog (macOS: Keybund brings its own dialog) |
| `wl-copy`, `xclip` or `xsel` | Linux, optional, CLI only: `keybund copy` puts the public key into the clipboard (the app has its own clipboard; macOS needs nothing) |

The doctor (`keybund doctor` or **Doctor** in the app) tells you if something is missing.

## Desktop app

=== "AppImage (any distribution)"

    ```sh
    chmod +x Keybund_*_amd64.AppImage
    ./Keybund_*_amd64.AppImage
    ```

    To get a menu entry, move the AppImage to `~/.local/bin/Keybund.AppImage` and copy
    [`packaging/keybund.desktop`](https://github.com/firsttris/keybund/blob/main/packaging/keybund.desktop)
    to `~/.local/share/applications/` (replace `@BIN_DIR@` with `~/.local/bin`), or use an AppImage
    manager such as Gear Lever.

=== "Debian, Ubuntu (.deb)"

    ```sh
    sudo apt install ./keybund_*_amd64.deb
    ```

    The app then shows up in the application menu as **Keybund**.

=== "macOS (.dmg)"

    Open `Keybund_*_universal.dmg` and drag **Keybund** into **Applications**.

    The app is not notarized by Apple yet, so macOS refuses to open it the first time. Open it once
    anyway: **System Settings → Privacy & Security → Open Anyway** (on macOS 14 and older also:
    right-click the app → **Open**). Or remove the download mark in a terminal:

    ```sh
    xattr -dr com.apple.quarantine /Applications/Keybund.app
    ```

    git and ssh come with the Xcode Command Line Tools; macOS offers to install them the first time
    `git` runs (or: `xcode-select --install`).

## Command line

```sh
tar -xzf keybund-cli-*-x86_64-linux.tar.gz        # macOS: keybund-cli-*-universal-macos.tar.gz
install -m 755 keybund ~/.local/bin/keybund
keybund --version
keybund doctor
```

`~/.local/bin` must be in your `PATH` (it is on most distributions; on macOS add
`export PATH="$HOME/.local/bin:$PATH"` to `~/.zshrc`, or install to `/usr/local/bin`). On macOS a
downloaded binary carries the download mark too: `xattr -d com.apple.quarantine ~/.local/bin/keybund`. The CLI is a single binary
that needs nothing besides `ssh` and `git`. All commands are described in
[Command line](command-line.md).

## Building from source

You need Rust (stable), Node.js 22 with pnpm, `just` and the WebKitGTK development files for Tauri.
On Fedora, Bazzite or any other immutable system the easiest way is a toolbox, so the host stays
clean:

```sh
git clone https://github.com/firsttris/keybund.git && cd keybund
packaging/toolbox.sh            # once: a Fedora toolbox with webkit2gtk, Rust, Node, pnpm, just
toolbox enter keybund-dev
just setup                      # frontend dependencies
just install                    # builds CLI and AppImage, installs both to ~/.local/bin with a menu entry
```

On Debian or Ubuntu the packages are
`libwebkit2gtk-4.1-dev libgtk-3-dev librsvg2-dev libayatana-appindicator3-dev` plus `build-essential`;
then the same `just setup && just install`.

On macOS you need the Xcode Command Line Tools, Rust, Node.js 22 with pnpm and `just`; then
`just setup` and `cd app && pnpm tauri build --bundles app` – the app is in
`target/release/bundle/macos/Keybund.app`. More in [Development](development.md).

## Updating

Download the new AppImage, `.deb`, `.dmg` or CLI and replace the old one. Keybund keeps its data in
`~/.config/keybund/profiles.toml`; a new version reads the old file and keeps settings it does not
know yet. After an update, open the app once or run `keybund apply` so the generated files follow
any change in the new version (the app shows pending changes on its own).

## Uninstalling

Keybund can take back everything it wrote **before** you delete the program:

=== "App"

    **Settings → Advanced → Remove Keybund** shows exactly what will be removed. Optionally restore `~/.ssh/config` and `~/.gitconfig` from the backups taken before
    Keybund's first change, and delete Keybund's own list of keys.

=== "CLI"

    ```sh
    keybund uninstall                    # removes generated files and the two Include lines
    keybund uninstall --restore-backups  # instead restores ~/.ssh/config and ~/.gitconfig from the backups
    keybund uninstall --purge            # also deletes ~/.config/keybund/profiles.toml
    ```

Your keys are never deleted – neither the ones Keybund created nor archived ones in
`~/.ssh/archive/`. Then remove the AppImage (or `sudo apt remove keybund`) and the CLI binary. With
`just install` from source, `just uninstall` removes the binaries and the menu entry.
