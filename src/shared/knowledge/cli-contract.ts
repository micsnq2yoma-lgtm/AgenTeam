export const KNOWLEDGE_COMMAND_FLAGS = {
  'kb/list': [],
  'kb/search': ['query', 'kb', 'tag', 'category', 'type', 'limit'],
  'kb/get': ['context'],
  'kb/read': ['page', 'heading', 'cursor'],
  'kb/add': ['file', 'fileValueMissing', 'kb'],
  'kb/meta/set': ['summaryFile', 'tags', 'category', 'type', 'date', 'entitiesFile', 'duplicateOf'],
  'kb/status': [],
} as const;

export type KnowledgeCommand = keyof typeof KNOWLEDGE_COMMAND_FLAGS;

export function isKnowledgeCommand(route: string): route is KnowledgeCommand {
  return Object.hasOwn(KNOWLEDGE_COMMAND_FLAGS, route);
}

export const KNOWLEDGE_CLI_HELP: Record<KnowledgeCommand | 'kb' | 'kb/meta', string> = {
  kb: `myagents kb — Use knowledge libraries bound to this Session

  list                         Show accessible libraries and their source counts
  search <query>               Search accessible library content with BM25
  get <chunk-ref>              Read an exact chunk and optional neighbors
  read <source-ref>            Read source Markdown by page, heading, or cursor
  add --file <path>            Import one regular file from this Workspace
  meta set <source-ref>        Update source metadata when the library allows writing
  status <job-id>              Inspect an import in an accessible library

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
User-edited fields retain their value. Results identify updated and locked fields individually.`,
  'kb/status': `myagents kb status <job-id> [--json]

Pass a job ID from myagents kb add. Returns the import and retrieval state for an accessible library.`,
};
