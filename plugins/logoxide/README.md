# logoxide skills

Two skills that let an AI coding assistant set logoxide up for you:

- **logoxide-app**: puts Bible text in an app you're writing, in TypeScript, Python, or Rust.
  It chooses where the text comes from (fetch.bible, eBible.org, or files your users upload),
  writes the loading, caching, and reading code, and knows the problems apps have run into.
- **logoxide-vault**: sets up `lx` and a vault of Bibles beside your notes. It covers importing
  translations, commentaries, and Strong's dictionaries, syncing the vault between devices, and
  connecting it to your editor (the language server, Neovim, Obsidian) and to Claude (the MCP
  server).

## Claude Code

```
/plugin install logoxide --marketplace Logoxide/logoxide
```

Or in two steps, or from a shell:

```sh
claude plugin marketplace add Logoxide/logoxide
claude plugin install logoxide@logoxide
```

Claude uses a skill when your request fits it, or you can name one: `/logoxide:logoxide-app`,
`/logoxide:logoxide-vault`. To get new versions, run `/plugin marketplace update`.

To use one skill without the plugin, copy its folder to `~/.claude/skills/` (for every project)
or to `.claude/skills/` in a repository:

```sh
git clone --depth 1 https://github.com/Logoxide/logoxide /tmp/logoxide
cp -r /tmp/logoxide/plugins/logoxide/skills/logoxide-app ~/.claude/skills/
```

## Other assistants

The skills follow the [Agent Skills](https://agentskills.io) format, using only `name` and
`description`, so other tools read them as they are. Copy a skill's folder to
`.agents/skills/` in a project, or to `~/.agents/skills/` for every project. Cursor and Codex
read both places, and Cursor also reads `~/.claude/skills/`.

## Contents

```
skills/logoxide-app/SKILL.md                       the workflow and the problems to avoid
skills/logoxide-app/references/api.md              the code, in TypeScript, Python, and Rust
skills/logoxide-app/references/sources.md          the publishers, their lists, CORS, licenses
skills/logoxide-app/references/recipes.md          links to the full recipes
skills/logoxide-vault/SKILL.md                     lx, a vault, syncing, problems
skills/logoxide-vault/references/integrations.md   commentaries, MCP, the language server, Neovim, Obsidian
```

The plugin's version is logoxide's (`scripts/check-versions.sh` checks it), and the skills
describe that release. The full documentation is in
[doc/apps.md](../../doc/apps.md) and [doc/recipes/](../../doc/recipes/).
