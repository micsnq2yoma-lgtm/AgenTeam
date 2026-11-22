import { readFileSync } from 'fs';
import { join } from 'path';

/** Workspace-relative rules file; `enabled: true` in its front matter opts in. */
export const PROACTIVE_RULES_FILE = join('memory', 'PROACTIVE_RULES.md');
export const PROACTIVE_SKILL = 'agenteam-proactive';

export function isProactiveEnabled(rulesText: string): boolean {
  const front = /^---\r?\n([\s\S]*?)\r?\n---/.exec(rulesText.replace(/^﻿/, ''));
  if (!front) return false;
  return front[1].split(/\r?\n/).some(line => /^enabled\s*:\s*["']?true["']?\s*$/i.test(line.trim()));
}

/**
 * Extra heartbeat instruction when the user opted in to proactive reminders;
 * null otherwise, which leaves the heartbeat exactly as it was.
 */
export function readProactiveHeartbeatNotice(workspacePath: string): string | null {
  let text: string;
  try {
    text = readFileSync(join(workspacePath, PROACTIVE_RULES_FILE), 'utf-8');
  } catch {
    return null;
  }
  if (!isProactiveEnabled(text)) return null;
  return `主动提醒已开启：本次心跳请使用 \`${PROACTIVE_SKILL}\` skill 做一次巡检（先运行它的 status 检查安静时段与每日上限）。没有值得现在告诉用户的事就回复 HEARTBEAT_OK。`;
}
