# Commentaries, dictionaries, and integrations

## Contents

- [Commentaries, Strong's, and cross-references](#commentaries-strongs-and-cross-references)
- [Where things are](#where-things-are)
- [Claude and other MCP clients](#claude-and-other-mcp-clients)
- [The language server](#the-language-server)
- [Neovim](#neovim)
- [Obsidian](#obsidian)

## Commentaries, Strong's, and cross-references

CrossWire's SWORD repository has public-domain commentaries, dictionaries, and the Treasury of
Scripture Knowledge; `lx search` finds them (`lx search henry`, `lx search strongs`).

```sh
lx import sword:crosswire/MHCC           # Matthew Henry's Concise Commentary
lx import sword:crosswire/StrongsGreek   # and StrongsHebrew
lx import sword:crosswire/TSK            # Treasury of Scripture Knowledge
```

`lx show` reads a package's folder; `lx info <id>` prints it on its `in:` line.

```sh
MHCC=$(lx info sword-mhcc | sed -n 's/^in: \(.*\) (.*/\1/p')
lx show "$MHCC" --notes "John 3:16" --stretch   # the note covering 3:16
lx show "$(lx info sword-strongsgreek | sed -n 's/^in: \(.*\) (.*/\1/p')" --entry G25   # ἀγαπάω …
lx show "$(lx info sword-tsk | sed -n 's/^in: \(.*\) (.*/\1/p')" --notes "John 3:16"     # Lu 2:14; Ro 5:8; …
```

- Commentaries that write one note for a section (Matthew Henry's) need `--stretch`, so a verse
  the commentary skips gets the note before it. Without it, `John 3:16` finds nothing.
- SWORD's TSK is a commentary whose notes are references: read it with `--notes`, not `--xref`
  (`--xref` is for packages with `xref/*.tsv` cross-references).
- Entries are found as `G25`, `G0025`, or `g25`, or by headword.
- `--format json` on any of them.

## Where things are

| | Default | Set by |
|---|---|---|
| Library (packages, shared by vaults) | `$XDG_DATA_HOME/logoxide` (`~/.local/share/logoxide`) | `LOGOXIDE_HOME`, config `library`, `--library` |
| Cache (per device) | `$XDG_CACHE_HOME/logoxide` | `LOGOXIDE_CACHE`, config `cache`, `--cache-dir` |
| Config | `~/.config/logoxide/config.toml` | `LOGOXIDE_CONFIG` (the file must exist) |

```toml
# ~/.config/logoxide/config.toml — every key optional
library = "~/Notes/.logoxide"     # inside the vault, so it syncs with the notes
catalog-max-age = "7d"
catalogs = ["https://example.org/catalog.toml"]
```

The language server, the MCP server, and `lx` read the same library and cache.

## Claude and other MCP clients

```sh
cargo install logoxide-mcp
claude mcp add logoxide -- logoxide-mcp --project ~/Notes                  # read-only
claude mcp add logoxide -- logoxide-mcp --project ~/Notes --allow-writes   # may install and import
```

Claude Desktop, `claude_desktop_config.json`:

```json
{
  "mcpServers": {
    "logoxide": { "command": "logoxide-mcp", "args": ["--project", "/home/me/Notes"] }
  }
}
```

Use an absolute path there, and the binary's full path (`~/.cargo/bin/logoxide-mcp`) if the
app doesn't see Cargo's bin folder. Run `lx install` in the vault first.

Tools: `search_catalogs`, `list_packages`, `package_info`, `read_passage`, `compare_passage`,
`map_reference`, `show_summary`, `diff_versions`, `cited_references`, `read_notes`,
`lookup_entry`, `cross_references`, and with `--allow-writes` `install` and `import` (only
entries of the vault's catalogs). Text from packages comes marked untrusted; passages carry
their license, and text that isn't known to be free is limited to 500 verses a call.

## The language server

```sh
cargo install logoxide-lsp
```

It runs over stdio as `logoxide-lsp`, for Markdown and text files: completion of references
(`jn 3:1` → `John 3:16`), hover with the passage, go to definition (the whole book), inlay
hints, document symbols, references across the workspace, code actions that insert a passage,
and diagnostics on references that don't exist.

It reads topos's `~/.config/topos/config.toml`. With no Bible settings, it uses the Bibles of
the vault open as the workspace folder (or the library's own project).

```toml
# ~/.config/topos/config.toml
default-bible = "BSB"
hover-bibles = ["KJV"]        # hover shows these too
commentaries = true            # the vault's commentaries' notes after the text
strongs = true                 # hovering G25 / H430 shows the entry
project = "~/Notes"            # when the workspace isn't the vault

[templates.callout]            # a code action: Insert callout
display = "> [!bible] {reference}\n{text}"
line_prefix = "> "
action = "insert"
```

Editors without a plugin: configure a language server with the command `logoxide-lsp` for
`markdown` and `text`, rooted at the folder containing `logoxide.toml`.

## Neovim

[logoxide.nvim](https://github.com/Logoxide/logoxide.nvim) starts the language server and adds
search and reading commands. With lazy.nvim:

```lua
{
  'Logoxide/logoxide.nvim',
  main = 'logoxide',
  build = ':LxBuild!',      -- builds logoxide-lsp and topos with Cargo
  ft = { 'markdown', 'text' },
  opts = {},                -- without `bible`, the language server uses the vault's Bibles
}
```

## Obsidian

[obsidian-logoxide](https://github.com/Logoxide/obsidian-logoxide) finds, searches, and
completes references in a vault. It isn't in Obsidian's community plugins yet; its README says
how to build it and copy it into `.obsidian/plugins/`.
