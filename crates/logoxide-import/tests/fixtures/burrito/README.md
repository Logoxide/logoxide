# Scripture Burrito fixtures

Real burritos, fetched 2026-10-19, cut down to small books; their files are checked by the MD5s
their `metadata.json` lists, so they're kept byte for byte (`.gitattributes`).

- `web/`: [bible-technology/sb_textTranslation](https://github.com/bible-technology/sb_textTranslation)
  at `b7c80a8f10be661b1b9f4c2de2d06b850b4487a4`, the specification's example of a source
  `textTranslation` burrito, written by Paratext 9.1: the World English Bible (public domain).
  Its ingredients cut to Titus and Jude, and `currentScope` and `localizedNames` with them; the
  files as they were, with the front matter's (`A0FRTengWEB14.SFM`), which the metadata doesn't
  list. Git has turned the repository's CRLFs into LFs, so none of its files has the size or MD5
  listed until its line endings are put back, as burritos kept in git will be
- `vandyke/`: [Proskomma/ar-textTranslation-New_Testament-vandyke](https://github.com/Proskomma/ar-textTranslation-New_Testament-vandyke)
  at `f2f32f1c723216ce61120ffa2082aad9aa48a188`, whole: Titus in Van Dyck's Arabic (public
  domain). Looser than the schema, as burritos in the wild are: its ingredients' paths leave out
  `ingredients/`, its USFM has no `mimeType`, and its copyright is a short statement
