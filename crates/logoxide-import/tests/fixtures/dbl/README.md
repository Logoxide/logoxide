# Digital Bible Library fixtures

- `bsb/`: the Berean Standard Bible's text release bundle (`317e03834065a236`, revision 1, made by
  Paratext 9.5), the one the hand-made `bsb` package came from; the BSB is public domain. Cut to
  Titus: `metadata.xml`'s book names, manifest, canonical content, and publication without the
  other books, and without the LDML, versification, and source resources; `release/styles.xml`
  is still listed but not here, as a file the import doesn't need. `release/USX_1/TIT.usx` and
  `license.xml` as they were, so they're kept byte for byte (`.gitattributes`)
