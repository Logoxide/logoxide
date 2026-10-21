---
name: logoxide-vault
description: Set up logoxide's `lx` command line and a vault of Bibles beside someone's notes — installing lx, finding translations in eBible.org, fetch.bible, CCEL, and CrossWire's SWORD lists, importing them, reading and comparing passages, locking and syncing a vault across devices, adding commentaries, Strong's dictionaries, and cross-references, and connecting the vault to editors and assistants (the logoxide language server, logoxide.nvim, the Obsidian plugin, and the logoxide MCP server for Claude). Use when someone wants Bibles in their notes, Obsidian vault, Neovim, or Markdown files, asks about lx, logoxide-lsp, or logoxide-mcp, or wants an assistant to read Bible passages from their own library.
---

# A vault of Bibles with lx

A **vault** is any folder of notes with a `logoxide.toml` (which Bibles it uses) and a
`logoxide.lock` (their exact text). Packages live in a shared **library**
(`$XDG_DATA_HOME/logoxide`, usually `~/.local/share/logoxide`), so vaults share one copy.

## Workflow

1. **Check what's there**: `lx --version`, `cargo --version`, and whether the notes folder
   already has a `logoxide.toml`. Ask where the notes are if it isn't obvious.
2. **Install** what's missing (Rust 1.89+ from https://rustup.rs if there's no `cargo`):
   ```sh
   cargo install logoxide-cli    # the `lx` command (the crate `lx` is someone else's)
   ```
3. **Make the vault** in the notes folder, then find and import translations. Show the user
   the search results with their licenses and let them choose:
   ```sh
   cd ~/Notes
   lx init
   lx search "berean"            # name, abbreviation, or language; every word must match
   lx import fetch:eng_bsb       # the import name lx search printed
   lx import fetch:eng_kjv
   lx read "John 3:16 BSB"
   lx compare "Psalm 51:1" --bibles KJV,BSB
   ```
4. **Sync**: commit or sync `logoxide.toml` and `logoxide.lock` with the notes. On another
   device, `lx install` gets exactly the locked text, or fails saying why. To sync the library
   itself (no downloads on other devices), point it inside the vault: `library = "~/Notes/.logoxide"`
   in `~/.config/logoxide/config.toml` (or `LOGOXIDE_HOME`).
5. **Connect the tools** the user uses (below), then verify one end to end: hover or read
   John 3:16 in their editor or assistant.

## Commands worth knowing

| | |
|---|---|
| `lx list` | what the vault uses, as locked |
| `lx info <id>` | where a package is, its books, whether its files still match the lock |
| `lx import --update` | make imports again from fresh downloads; says what changed (versions rise by it) |
| `lx update [id]` | newer versions of git packages the requirements allow |
| `lx disable <id>` / `lx enable <id>` | turn one off without removing it |
| `lx remove <id>` | out of the vault (files stay in the library); `lx clean --remove` deletes unused versions |
| `lx search --refresh` / `--offline` | catalogs are kept 7 days (`catalog-max-age`) |
| `lx add github:owner/repo` | a package published as a git repository |
| `lx add --path "Bibles/Mine"` | a folder used in place |
| `lx import ./bundle.zip` | a Scripture Burrito or Digital Bible Library bundle |
| `--format json` | on list, info, search, read, compare, show, … |

Completion: `source <(COMPLETE=bash lx)` (zsh, fish likewise). Without a `logoxide.toml` in
the current folder, lx uses the library's own project (`--global`).

More on commentaries, dictionaries, and cross-references, and every integration's settings:
[references/integrations.md](references/integrations.md).

## Integrations, briefly

- **Claude (MCP)**: `cargo install logoxide-mcp`, then
  `claude mcp add logoxide -- logoxide-mcp --project ~/Notes` (read-only; add `--allow-writes`
  to let it install and import). Tools: `read_passage`, `compare_passage`, `search_catalogs`,
  `read_notes`, `lookup_entry`, `cross_references`, and more. Run `lx install` first.
- **Any editor with LSP**: `cargo install logoxide-lsp`; it runs over stdio as `logoxide-lsp`
  for Markdown and text files: completion of references, hover with the passage's text,
  go to definition (the whole book), diagnostics for references that don't exist. It uses the
  vault's Bibles when the workspace folder has a `logoxide.toml`.
- **Neovim**: the `Logoxide/logoxide.nvim` plugin (lazy.nvim spec in integrations.md).
- **Obsidian**: `obsidian-logoxide` (github.com/Logoxide/obsidian-logoxide); not in Obsidian's
  community plugins yet: its README says how to build and install it.

## Good to tell the user

- **Licenses are shown, never enforced.** Many modern translations allow personal use only;
  check before sharing text. Public-domain choices: BSB, KJV, WEB, ASV, YLT.
- Imports start at version 1.0.0; the publisher's own version is kept in `[package.upstream]`.
- A reference names a Bible by the abbreviation its publisher gives (`lx list` shows them):
  fetch.bible's KJV is `KJV`, eBible.org's is `KJV2006`. Two publishers' copies of one
  translation are different packages (`fetch-eng_kjv`, `ebible-eng-kjv2006`), and lx warns
  when a vault has both; keep one.
- Nothing here needs an account or a server; everything is files.

## When something fails

- `lx install` says a checksum differs: the files changed since locking; `lx info <id>` shows
  which. `lx import --update` or `lx update` re-locks deliberately.
- A search finds nothing offline: `lx search --offline` uses kept catalogs only; try again online.
- Hover shows no text: the editor's workspace folder must be the vault (or set `project` in
  `~/.config/topos/config.toml`), and the packages installed (`lx install`).
