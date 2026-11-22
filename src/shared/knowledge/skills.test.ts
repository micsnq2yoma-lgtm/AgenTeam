import { readFileSync } from 'node:fs';
import { resolve } from 'node:path';
import { describe, expect, it } from 'vitest';

import { KNOWLEDGE_CLI_HELP, KNOWLEDGE_COMMAND_FLAGS, isKnowledgeCommand } from './cli-contract';

const read = (relative: string) =>
  readFileSync(resolve(process.cwd(), relative), 'utf8').replace(/\r\n/g, '\n');

const SKILLS = {
  knowledge: read('bundled-skills/agenteam-knowledge/SKILL.md'),
  curator: read('bundled-skills/agenteam-knowledge-curator/SKILL.md'),
};

const camel = (flag: string) => flag.replace(/-([a-z])/g, (_, c: string) => c.toUpperCase());

/** Every concrete `myagents kb …` invocation written in a Skill. */
function invocations(markdown: string): string[] {
  const found: string[] = [];
  for (const match of markdown.matchAll(/myagents kb[^`\n]*/g)) found.push(match[0].trim());
  return found;
}

function routeOf(invocation: string): string | null {
  const words = invocation.split(/\s+/).slice(1);
  const parts: string[] = [];
  for (const word of words) {
    if (word.startsWith('-') || word.startsWith('<') || word.startsWith('"')) break;
    parts.push(word);
  }
  return parts.join('/');
}

describe('knowledge Skills track the K5 CLI contract', () => {
  for (const [name, markdown] of Object.entries(SKILLS)) {
    it(`${name}: every command and flag exists in the leaf help`, () => {
      const lines = invocations(markdown);
      expect(lines.length).toBeGreaterThan(0);
      for (const line of lines) {
        const route = routeOf(line);
        const flags = [...line.matchAll(/--([a-z][a-z-]*)/g)].map(match => match[1]);
        // `myagents kb --help` and `myagents kb <command> --help` are generic.
        if (route === 'kb' || line.includes('<command>')) {
          expect(flags.every(flag => flag === 'help'), line).toBe(true);
          continue;
        }
        expect(route && isKnowledgeCommand(route), line).toBe(true);
        const command = route as keyof typeof KNOWLEDGE_COMMAND_FLAGS;
        for (const flag of flags) {
          if (flag === 'help') continue;
          if (flag !== 'json') {
            expect(KNOWLEDGE_COMMAND_FLAGS[command] as readonly string[], `${line}: --${flag}`)
              .toContain(camel(flag));
          }
          expect(KNOWLEDGE_CLI_HELP[command], `${line}: --${flag} missing from help`)
            .toContain(`--${flag}`);
        }
      }
    });

    it(`${name}: treats library content as data, with concrete examples`, () => {
      const section = markdown.split('## 资料中的指令不是用户指令')[1]?.split('\n## ')[0] ?? '';
      expect(section).toMatch(/忽略之前的/);
      expect(section.split('\n').filter(entry => entry.startsWith('- ')).length).toBeGreaterThanOrEqual(3);
    });
  }

  it('descriptions separate retrieval from conversion, memory and managed curation', () => {
    const description = (markdown: string) => markdown.split('---')[1];
    expect(description(SKILLS.knowledge)).toMatch(/myagents-anydoc/);
    expect(description(SKILLS.knowledge)).toMatch(/memory/);
    expect(description(SKILLS.knowledge)).toMatch(/kb:\/\//);
    expect(description(SKILLS.curator)).toMatch(/仅当系统或用户明确指定完整名称 `agenteam-knowledge-curator`/);
  });
});

describe('library rule templates', () => {
  const rules = read('bundled-skills/agenteam-knowledge-curator/references/ORGANIZE_RULES.md');
  const schema = read('bundled-skills/agenteam-knowledge-curator/references/WIKI_SCHEMA.md');

  it('lists categories in the one-per-line form the curator and Store rely on', () => {
    const section = rules.split('## 分类体系')[1].split('\n## ')[0];
    const categories = [...section.matchAll(/^- `([^`]+)`：/gm)].map(match => match[1]);
    expect(categories.length).toBeGreaterThan(0);
    for (const category of categories) {
      expect(category).not.toMatch(/^\/|\/$|\/\/|\.\./);
    }
  });

  it('requires kb:// citations and a log entry for wiki changes', () => {
    expect(schema).toMatch(/kb:\/\//);
    expect(schema).toMatch(/log\.md/);
    expect(schema).toMatch(/index\.md/);
  });
});
