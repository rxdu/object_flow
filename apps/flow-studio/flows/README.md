# The workspace

The Flow Studio edits flows here, never the design's example modules in `docs/design/flow-format/examples/`, which are the corpus the repository's checkers hold the design to. A design module is copied here to be edited. Each `<module>.yaml` may have a `<module>.examples.yaml` beside it, to which the studio appends a session saved as an example once the flow checker passes it (`docs/design/flow-studio.md` §3.1, §5). The directory is set by `workspace` in `../config.json`.
