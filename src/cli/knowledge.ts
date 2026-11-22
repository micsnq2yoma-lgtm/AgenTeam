import {
  isKnowledgeBaseId,
  isKnowledgeJobId,
  parseKnowledgeRef,
} from '../shared/knowledge';
import type { KnowledgeCommand } from '../shared/knowledge/cli-contract';

type Flags = Record<string, unknown>;

function value(raw: unknown, spelling: string): string | undefined {
  if (raw === undefined) return undefined;
  if (typeof raw !== 'string' || !raw.trim()) throw new Error(`${spelling} requires a value.`);
  return raw;
}

function values(raw: unknown, spelling: string): string[] {
  if (raw === undefined) return [];
  const items = Array.isArray(raw) ? raw : [raw];
  return items.map(item => {
    const parsed = value(item, spelling);
    if (parsed === undefined) throw new Error(`${spelling} requires a value.`);
    return parsed;
  });
}

function integer(raw: unknown, spelling: string, min: number, max: number): number | undefined {
  const text = value(raw, spelling);
  if (text === undefined) return undefined;
  const number = Number(text);
  if (!/^\d+$/u.test(text) || !Number.isInteger(number) || number < min || number > max) {
    throw new Error(`${spelling} must be an integer from ${min} to ${max}.`);
  }
  return number;
}

function oneArgument(args: readonly string[], label: string): string {
  if (args.length !== 1 || !args[0].trim()) throw new Error(`Expected exactly one ${label}.`);
  return args[0];
}

function reference(args: readonly string[], chunk: boolean): string {
  const input = oneArgument(args, chunk ? '<chunk-ref>' : '<source-ref>');
  const parsed = parseKnowledgeRef(input);
  if (!parsed || Boolean(parsed.chunkId) !== chunk) throw new Error('Expected a valid kb:// reference.');
  return input;
}

function libraries(raw: unknown): string[] {
  const ids = values(raw, '--kb');
  if (!ids.every(isKnowledgeBaseId)) throw new Error('--kb requires a knowledge base ID.');
  return ids;
}

export function buildKnowledgeInput(
  route: KnowledgeCommand,
  args: readonly string[],
  flags: Flags,
): Record<string, unknown> {
  switch (route) {
    case 'kb/list':
      if (args.length) throw new Error('kb list takes no positional arguments.');
      return {};
    case 'kb/search':
      return {
        queries: [oneArgument(args, '<query>'), ...values(flags.query, '--query')],
        kbIds: libraries(flags.kb),
        filters: {
          tag: value(flags.tag, '--tag'),
          category: value(flags.category, '--category'),
          docType: value(flags.type, '--type'),
        },
        limit: integer(flags.limit, '--limit', 1, 20),
      };
    case 'kb/get':
      return { reference: reference(args, true), context: integer(flags.context, '--context', 0, 3) };
    case 'kb/read': {
      const page = integer(flags.page, '--page', 1, 0xffff_ffff);
      const heading = value(flags.heading, '--heading');
      if (page !== undefined && heading !== undefined) throw new Error('Use either --page or --heading.');
      return { reference: reference(args, false), page, heading, cursor: value(flags.cursor, '--cursor') };
    }
    case 'kb/add': {
      const files = values(flags.file, '--file');
      const ids = libraries(flags.kb);
      if (args.length || flags.fileValueMissing || files.length !== 1 || ids.length > 1) {
        throw new Error('Use one --file and at most one --kb.');
      }
      return { file: files[0], kbId: ids[0] };
    }
    case 'kb/meta/set': {
      const fields = {
        summaryFile: value(flags.summaryFile, '--summary-file'),
        tags: value(flags.tags, '--tags')?.split(',').map(tag => tag.trim()),
        category: value(flags.category, '--category'),
        docType: value(flags.type, '--type'),
        date: value(flags.date, '--date'),
        entitiesFile: value(flags.entitiesFile, '--entities-file'),
        duplicateOf: flags.duplicateOf === undefined ? undefined
          : reference(values(flags.duplicateOf, '--duplicate-of'), false),
      };
      if (Object.values(fields).every(field => field === undefined)) {
        throw new Error('Provide at least one metadata field.');
      }
      return { reference: reference(args, false), ...fields };
    }
    case 'kb/status': {
      const jobId = oneArgument(args, '<job-id>');
      if (!isKnowledgeJobId(jobId)) throw new Error('Expected a knowledge import job ID.');
      return { jobId };
    }
  }
}

export function knowledgeContentBlock(reference: string, content: string): string {
  if (!parseKnowledgeRef(reference)) throw new Error('Invalid knowledge reference in response.');
  const safe = content.replaceAll('&', '&amp;').replaceAll('<', '&lt;').replaceAll('>', '&gt;');
  return `<kb-content ref="${reference}">\n${safe}\n</kb-content>`;
}

export function printKnowledgeResult(action: string, data: Record<string, unknown>): void {
  if (action === 'read' && typeof data.reference === 'string' && typeof data.content === 'string') {
    console.log(knowledgeContentBlock(data.reference, data.content));
    if (typeof data.nextCursor === 'string') console.log(`Next cursor: ${data.nextCursor}`);
    return;
  }
  const snippets = action === 'search' ? data.hits : action === 'get' ? data.chunks : undefined;
  if (Array.isArray(snippets)) {
    for (const snippet of snippets) {
      if (snippet && typeof snippet.reference === 'string' && typeof snippet.snippet === 'string') {
        console.log(knowledgeContentBlock(snippet.reference, snippet.snippet));
      }
    }
    if (snippets.length) return;
  }
  console.log(JSON.stringify(data, null, 2));
}
