# CCEL and Door43 research fixtures (saved 2026-10-10, for PLAN-NEXT.md L.2 and L.3)

Fetched live from ccel.org on 2026-10-10 by the phase L agent, for the importer to be written
(nothing reads these yet):

- `gen-COM.html`: `https://ccel.org/index/genre/gen-COM`, "All Works in Genre 'Commentaries'"
  (213 entries, one page): each `<p class="List2"><a href="/ccel/<author>/<work>.html">Title</a>
  <span class="authorName">(<a href="/ccel/<author>">Name</a>)</span> (Language) <b>(classic)</b></p>`
- `gen-BIB.html`: `https://ccel.org/index/genre/gen-BIB`, the Bibles (42), the same shape
- `mhc1-head.xml`, `calcom01-head.xml`: the first 40 KB of Matthew Henry's `ccel/henry/mhc1.xml`
  and Calvin's `ccel/calvin/calcom01.xml` (7.5 MB and 1.8 MB whole): commentaries mark sections
  with empty `<scripCom type="Commentary" passage="Ge 1:1-2" parsed="|Gen|1|1|1|2"
  osisRef="Bible:Gen.1.1-Gen.1.2"/>` milestones (never `<scripture parsed=…/>`); chapter-level
  ones are `parsed="|Gen|1|0|0|0"`; books and chapters are `<div1 title="Genesis" id="Gen">`
  and `<div2 title="Chapter I">`

`../door43/manifest-docs.txt` is the text of
`https://resource-container.readthedocs.io/en/latest/manifest.html` (the `manifest.yaml`
shape: `dublin_core` with `title`, `language: {identifier, title, direction}`, `rights`,
`version`, `publisher`, `contributor`, …; `projects` each `identifier`, `path`, `title`,
`versification`). git.door43.org itself timed out from the owner's machine on 2026-10-10 and
2026-10-11 (catalog API, swagger, and archive URLs), so the DCS catalog JSON shape is unverified.
