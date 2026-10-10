# Door43 fixtures (PLAN-NEXT.md L.2)

git.door43.org didn't answer from the owner's machine on 2026-10-10 or 2026-10-11 (its catalog
API, swagger, and archive URLs all timed out; `api.door43.org` doesn't resolve), so:

- `catalog-search.json` is **written from memory**, not fetched: the shape of DCS's catalog API
  v5, `https://git.door43.org/api/v1/catalog/search?subject=Bible&stage=prod`
  (`{"ok": true, "data": [{owner, name, full_name, title, language, language_title,
  language_direction, subject, branch_or_tag_name, release.tag_name, tarbar_url, metadata_url,
  ingredients[]}]}`). The entries are made up in that shape (one isn't a Bible, one has no
  `tarbar_url`). Check it against the live API when it answers
- `manifest-docs.txt` is the text of
  `https://resource-container.readthedocs.io/en/latest/manifest.html`, fetched live: the
  `manifest.yaml` shape (`dublin_core` with `title`, `language: {identifier, title, direction}`,
  `rights`, `version`, …; `projects`, each `identifier`, `path`, `title`, `versification`),
  which `door43.rs`'s tests copy
