# Wiki Schema

This file controls how the curator agent maintains this knowledge base's wiki. The wiki is a synthesized topic layer above raw sources so both you and agents can grasp the library quickly. You can edit this file at any time.

## Pages

| Page | Purpose |
|---|---|
| `index.md` | Library overview: purpose, list of key topics (linked to topic pages), and recent updates |
| `topics/<topic>.md` | One page per topic synthesizing multiple sources |
| `log.md` | Append-only changelog of each curation round |

Use short noun phrases for topic filenames, for example `topics/product-pricing.md`.

## Topic Page Structure

```markdown
# <Topic Name>

<One-paragraph overview of this topic>

## Key Points

- <Finding>. [Source](kb://<kb-id>/<source-id>#<chunk-id>)

## Related Topics

- [<Topic Name>](<topic-file>.md)

## Open Questions

- <Contradictions, outdated facts, or gaps across sources>
```

## Citations

- Every claim must include at least one `kb://` citation pointing to the supporting raw source chunk. Do not write unsupported claims to the wiki.
- Cite raw sources only; do not cite other wiki pages as primary evidence.
- When sources disagree, list each side with its citation under "Open Questions" instead of guessing which one is right.

## Links

- Link between wiki pages using relative Markdown links, for example `[Product Pricing](product-pricing.md)`.
- Whenever you create a topic page, add a link to it in `index.md`; do not leave orphan pages with no incoming links.

## Log

Append one entry to `log.md` after each curation round:

```markdown
## <YYYY-MM-DD>

- Added: <page> (<reason>)
- Updated: <page> (<reason>)
- Open questions: <count>
```
