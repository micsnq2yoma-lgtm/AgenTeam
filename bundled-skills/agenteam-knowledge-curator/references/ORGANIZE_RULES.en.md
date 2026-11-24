# Organization Rules

This file controls how the curator agent generates summaries, tags, and categories for sources in this knowledge base. You can edit it at any time; the next curation round follows your changes.

## Categories

Categories appear as virtual folders in the Knowledge Base UI, separated by `/`. The curator agent only assigns categories listed below; if it needs a new category, it can only propose one for you to confirm in the UI.

One category per line, formatted as ``- `category/path`: description``:

- `Reference`: manuals, specifications, tutorials, whitepapers, and long-lived reference material
- `Projects`: plans, status updates, and deliverables tied to a specific project
- `Meetings`: meeting notes, discussion logs, and interview transcripts
- `Contracts`: contracts, agreements, quotes, and terms
- `Reports`: analysis reports, weekly updates, and research findings
- `Other`: sources that do not fit an existing category yet

## Tags

- Tags are flat keywords for cross-category retrieval. Assign 3–8 tags per source.
- Reuse existing tags whenever possible; keep only one term for synonyms (for example, pick either "client" or "customer").
- Use nouns or short noun phrases rather than full sentences.

## Document Types

Allowed values: `contract`, `report`, `meeting-notes`, `proposal`, `manual`, `paper`, `spreadsheet`, `presentation`, `note`, `other`.

## Summary

- Write 1–3 sentences per source stating what it is, what it covers, and who it helps.
- State only facts present in the source without adding opinions.

## Exclusions

Skip these sources during curation (do not generate metadata or write them to the wiki):

- (None yet)
