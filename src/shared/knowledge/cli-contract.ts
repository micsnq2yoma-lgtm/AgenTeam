export const KNOWLEDGE_COMMAND_FLAGS = {
  'kb/list': [],
  'kb/search': ['query', 'kb', 'tag', 'category', 'type', 'limit'],
  'kb/get': ['context'],
  'kb/read': ['page', 'heading', 'cursor'],
  'kb/add': ['file', 'fileValueMissing', 'kb'],
  'kb/meta/set': ['summaryFile', 'tags', 'category', 'type', 'date', 'entitiesFile', 'duplicateOf'],
  'kb/status': [],
  'kb/curation/context': ['kb'],
  'kb/wiki/write': ['kb', 'path', 'file', 'fileValueMissing'],
  'kb/wiki/log': ['kb', 'message'],
  'kb/wiki/lint-report': ['kb', 'file', 'fileValueMissing'],
  'kb/taxonomy/propose': ['kb', 'category', 'reason'],
} as const;

export type KnowledgeCommand = keyof typeof KNOWLEDGE_COMMAND_FLAGS;

export function isKnowledgeCommand(route: string): route is KnowledgeCommand {
  return Object.hasOwn(KNOWLEDGE_COMMAND_FLAGS, route);
}

export const KNOWLEDGE_CLI_HELP: Record<KnowledgeCommand | 'kb' | 'kb/meta' | 'kb/curation' | 'kb/wiki' | 'kb/taxonomy', string> = {
  kb: `myagents kb — Use knowledge libraries bound to this Session

  list                         Show accessible libraries and their source counts
  search <query>               Search accessible library content with BM25
  get <chunk-ref>              Read an exact chunk and optional neighbors
  read <source-ref>            Read source Markdown by page, heading, or cursor
  add --file <path>            Import one regular file from this Workspace
  meta set <source-ref>        Update source metadata when the library allows writing
  status <job-id>              Inspect an import in an accessible library
  curation context --kb <id>   Read library rules and the authorized curation batch
  wiki write|log|lint-report   Maintain the library's wiki
  taxonomy propose             Propose a category for user confirmation

Run myagents kb <command> --help for the full syntax. Requires a live AgenTeam Session.`,
  'kb/list': `myagents kb list [--json]

Lists accessible libraries, their purpose, retrieval mode, and source count.
Run from the Session whose libraries you want to access.`,
  'kb/search': `myagents kb search "<query>" [--query "<additional query>"]... [--kb <id>]...
                   [--tag <tag>] [--category <path>] [--type <type>] [--limit <n>] [--json]

No --kb searches all libraries available to this Session. Tag, category and type filter exact values.
Supply at most 16 queries, each up to 4096 UTF-8 bytes. Limit is 1–20 (default 8).
Results contain a snippet and a kb:// reference.`,
  'kb/get': `myagents kb get <chunk-ref> [--context <n>] [--json]

Pass a kb:// reference with a chunk ID. Context is 0–3 adjacent chunks on each side (default 0).
If the chunk was edited, the response identifies the source and reports that the fragment is unavailable.`,
  'kb/read': `myagents kb read <source-ref> [--page <n> | --heading "<title>"] [--cursor <cursor>] [--json]

Pass a kb:// source reference. Page is a positive number and heading is an exact title.
Returns up to 64 KiB of Markdown. Repeat the request with the same source and selector,
plus the supplied nextCursor, to read the following slice.`,
  'kb/add': `myagents kb add --file <path> [--kb <id>] [--json]

Imports one regular file from the Session's Workspace. Links and paths outside the Workspace are rejected.
Choose --kb if the Session has more than one accessible library. Writing requires library permission.
The receipt includes an import job ID for myagents kb status.`,
  'kb/meta': `myagents kb meta set <source-ref> [options]

Run myagents kb meta set --help for the available fields and file limits.`,
  'kb/meta/set': `myagents kb meta set <source-ref> [--summary-file <path>] [--tags <a,b>]
                     [--category <path>] [--type <type>] [--date <YYYY-MM-DD>]
                     [--entities-file <path>] [--duplicate-of <source-ref>] [--json]

Specify at least one field. Summary is UTF-8 text and entities are a JSON array of strings.
Each file must be a regular file within the Session's Workspace and at most 64 KiB.
User-edited fields retain their value. Results identify updated and rejected fields individually.
For KB_CATEGORY_NOT_ALLOWED, use myagents kb taxonomy propose.`,
  'kb/status': `myagents kb status <job-id> [--json]

Pass a job ID from myagents kb add. Returns the import and retrieval state for an accessible library.`,
  'kb/curation': `myagents kb curation context --kb <id> [--json]`,
  'kb/curation/context': `myagents kb curation context --kb <id> [--json]

Reads rules, the wiki schema and index, and approved and pending categories. Missing files are null.
Only the library's active curation Session receives the batch of at most 50 pending sources.`,
  'kb/wiki': `myagents kb wiki write|log|lint-report --help`,
  'kb/wiki/write': `myagents kb wiki write --kb <id> --path <relative.md> --file <local.md> [--json]

Publish one wiki page from a regular Markdown file in this Session's Workspace.
Limit: 1 MiB per page and 2,000 pages per library.`,
  'kb/wiki/log': `myagents kb wiki log --kb <id> --message "<change summary>" [--json]

Append a dated curation entry to wiki/log.md.`,
  'kb/wiki/lint-report': `myagents kb wiki lint-report --kb <id> --file <report.json> [--json]

Submit a version 1 wiki lint report from a regular JSON file in this Session's Workspace.`,
  'kb/taxonomy': `myagents kb taxonomy propose --help`,
  'kb/taxonomy/propose': `myagents kb taxonomy propose --kb <id> --category <path> --reason "<reason>" [--json]

Propose a category for the active curation Task's library. Only a user can approve it.`,
};
