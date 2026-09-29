# forgelib

A minimal Foundry library cache manager.

Instead of re-downloading the same Solidity library every time you start a new project, forgelib clones it once into a local OS cache and lets you copy (or symlink) it into any project's `lib/` folder on demand. It also keeps `remappings.txt` up to date automatically.

---

## Why bother

Foundry's built-in `forge install` fetches libraries straight into the project. If you work across many projects you end up cloning the same repos over and over. forgelib separates the download step from the install step so you only hit the network once per library version.

---

## Requirements

- Rust (to build from source)
- Git must be on your `PATH`

---

## Installation

Clone the repo and build:

```
git clone https://github.com/your-username/forgelib
cd forgelib
cargo install --path .
```
OR

```
git clone https://github.com/your-username/forgelib
cd forgelib
cargo build --release
```

Then move the binary somewhere on your PATH, for example:

```
cp target/release/forgelib ~/.local/bin/
```

Confirm the installation:

```
which forgelib
forgelib --help
```

---

## How it works

There are four commands: `fetch`, `use`, `list`, and `remove`.

The general flow is:

1. **fetch** - download a library into the cache (only happens once)
2. **use** - copy it from the cache into your current project's `lib/` folder

---

## Commands

### fetch

Downloads a library and stores it in the OS cache under `forgelib/<name>/<version>/`. If that exact name and version already exist in the cache the command does nothing.

```
forgelib fetch <repo_url> <name> <version>
```

**Example** - cache OpenZeppelin contracts v5.0.2:

```
forgelib fetch https://github.com/OpenZeppelin/openzeppelin-contracts openzeppelin-contracts v5.0.2
```

**Example** - cache forge-std:

```
forgelib fetch https://github.com/foundry-rs/forge-std forge-std v1.8.2
```

The `.git` directory is stripped out after cloning, so only the source files end up in the cache.

---

### use

Copies (or symlinks) a cached library into `./lib/<name>` inside your current working directory, and appends the correct remapping to `remappings.txt`. Run this from the root of your Foundry project.

```
forgelib use <name> <version>
```

**Example** - install OpenZeppelin into the current project:

```
forgelib use openzeppelin-contracts v5.0.2
```

After this your `remappings.txt` will have a line like:

```
openzeppelin-contracts/=lib/openzeppelin-contracts/contracts/
```

The remapping path (`src/` or `contracts/`) is detected automatically based on the library's directory layout. If neither exists, it falls back to the library root.

**Symlink instead of copy (Unix only)**

If you want to avoid duplicating files across projects, pass `--link`:

```
forgelib use forge-std v1.8.2 --link
```

This creates a symlink at `./lib/forge-std` pointing at the cached copy. Any change to the cache will be visible in the project immediately, so only use this if you know what you are doing.

---

### list

Shows everything currently in the cache.

```
forgelib list
```

Example output:

```
  forge-std
      --v1.8.2
  openzeppelin-contracts
      --v5.0.1
      --v5.0.2
```

---

### remove

Removes a library version from the cache. You will be shown exactly what is about to be deleted and asked to confirm before anything is removed.

Remove a specific version:

```
forgelib remove openzeppelin-contracts v5.0.1
```

Remove all cached versions of a library at once:

```
forgelib remove openzeppelin-contracts
```

Pressing Enter or typing anything other than `y` / `yes` at the prompt cancels the operation. Nothing is removed unless you explicitly confirm.

---

## Input rules

`name` and `version` must only contain ASCII letters, digits, `-`, `_`, and `.`. They cannot be empty, contain spaces, path separators, `..`, or start with `.` or `-`.

`repo_url` must be a plain `https://github.com/owner/repo` URL. Query strings, fragments, and non-GitHub URLs are rejected.

---

## Cache location

The cache lives inside the standard OS cache directory:

| OS      | Default path                                     |
|---------|--------------------------------------------------|
| macOS   | `~/Library/Caches/forgelib/`                     |
| Linux   | `~/.cache/forgelib/`                             |
| Windows | `C:\Users\<user>\AppData\Local\forgelib\cache\`  |

Each library is stored as `<cache_root>/<name>/<version>/`.

---

## Running the tests

```
cargo test
```

---

## License

MIT
