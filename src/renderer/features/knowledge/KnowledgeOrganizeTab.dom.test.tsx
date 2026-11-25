import { fireEvent, render, screen, waitFor, within } from '@testing-library/react';
import { beforeEach, describe, expect, it, vi } from 'vitest';

import { ToastProvider } from '@/components/Toast';
import { i18n } from '@/i18n';

import KnowledgeOrganizeTab, { isOrganizeItemDefaultSelected } from './KnowledgeOrganizeTab';
import type { KnowledgeBaseRecord, OrganizePlan } from './types';

const mocks = vi.hoisted(() => ({
  invoke: vi.fn(),
}));

vi.mock('@tauri-apps/api/core', () => ({ invoke: mocks.invoke }));

const tl = (key: string, vars?: Record<string, unknown>) =>
  String(i18n.t(`launcher:${key}`, vars));

function makeKb(over: Partial<KnowledgeBaseRecord> = {}): KnowledgeBaseRecord {
  return {
    id: 'kb_111111111111',
    name: '工程文档',
    description: '架构与发布资料',
    curatorAgentId: 'agent_1',
    curationMode: 'manual',
    curationIntervalMinutes: 4320,
    agentWriteAccess: 'curator-only',
    wikiEnabled: true,
    pendingCategories: [],
    linkedFolders: [
      {
        path: 'C:/docs/work',
        organizeAllowed: true,
        gitConfirmed: true,
        isGitWorkTree: true,
      },
    ],
    ...over,
  };
}

function makePendingPlan(): OrganizePlan {
  return {
    id: 'plan_20261125_0123456789ab',
    kbId: 'kb_111111111111',
    status: 'pending_review',
    createdAt: '2026-11-25T05:00:00Z',
    expiresAt: '2026-12-02T05:00:00Z',
    updatedAt: '2026-11-25T05:00:00Z',
    items: [
      {
        id: 'op_1',
        kind: 'mkdir',
        folderRoot: 'C:/docs/work',
        targetRel: 'archive',
        reason: '为归档文档新建目录',
        affectedMarkdownLinks: 0,
        defaultSelected: true,
        approved: true,
        status: 'pending',
      },
      {
        id: 'op_2',
        kind: 'move',
        folderRoot: 'C:/docs/work',
        sourceRel: 'notes.md',
        targetRel: 'archive/notes.md',
        reason: '归档旧会议记录',
        affectedMarkdownLinks: 0,
        defaultSelected: true,
        approved: true,
        status: 'pending',
      },
      {
        id: 'op_3',
        kind: 'rename',
        folderRoot: 'C:/docs/work',
        sourceRel: 'guide.md',
        targetRel: 'handbook.md',
        reason: '统一文档命名规范',
        affectedMarkdownLinks: 2,
        defaultSelected: false,
        approved: false,
        status: 'pending',
      },
    ],
  };
}

describe('KnowledgeOrganizeTab (K15)', () => {
  beforeEach(() => {
    mocks.invoke.mockReset();
  });

  it('defaults to unchecking items that affect Markdown links while keeping safe items checked', () => {
    const plan = makePendingPlan();
    expect(isOrganizeItemDefaultSelected(plan.items[0])).toBe(true);
    expect(isOrganizeItemDefaultSelected(plan.items[2])).toBe(false);
    // Even if backend passed defaultSelected=true with affectedMarkdownLinks > 0, helper still returns false.
    expect(
      isOrganizeItemDefaultSelected({ ...plan.items[2], defaultSelected: true, affectedMarkdownLinks: 1 }),
    ).toBe(false);

    render(
      <ToastProvider>
        <KnowledgeOrganizeTab kb={makeKb()} plans={[plan]} loading={false} onReload={vi.fn()} />
      </ToastProvider>,
    );

    const op1Checkbox = within(screen.getByTestId('organize-item-op_1')).getByRole('checkbox');
    const op2Checkbox = within(screen.getByTestId('organize-item-op_2')).getByRole('checkbox');
    const op3Checkbox = within(screen.getByTestId('organize-item-op_3')).getByRole('checkbox');

    expect(op1Checkbox).toBeChecked();
    expect(op2Checkbox).toBeChecked();
    expect(op3Checkbox).not.toBeChecked();

    // Risk badges: Markdown links + Git work tree
    const op3 = screen.getByTestId('organize-item-op_3');
    expect(op3.querySelector('[data-risk="markdown-links"]')).not.toBeNull();
    expect(op3.querySelector('[data-risk="git"]')).not.toBeNull();
  });

  it('supports toggling items, editing target override, accept-all, and approve+execute flow', async () => {
    const plan = makePendingPlan();
    const onReload = vi.fn().mockResolvedValue(undefined);
    mocks.invoke.mockImplementation(async (cmd: string) => {
      if (cmd === 'cmd_kb_organize_approve') {
        return { ...plan, status: 'approved' };
      }
      if (cmd === 'cmd_kb_organize_execute') {
        return { ...plan, status: 'completed' };
      }
      return null;
    });

    render(
      <ToastProvider>
        <KnowledgeOrganizeTab kb={makeKb()} plans={[plan]} loading={false} onReload={onReload} />
      </ToastProvider>,
    );

    // Click "Accept all" -> op_3 becomes checked too (3 selected)
    fireEvent.click(screen.getByRole('button', { name: tl('knowledge.organize.acceptAll') }));
    expect(within(screen.getByTestId('organize-item-op_3')).getByRole('checkbox')).toBeChecked();

    // Uncheck op_1
    fireEvent.click(within(screen.getByTestId('organize-item-op_1')).getByRole('checkbox'));
    expect(within(screen.getByTestId('organize-item-op_1')).getByRole('checkbox')).not.toBeChecked();

    // Edit target name of op_2
    const targetInput = within(screen.getByTestId('organize-item-op_2')).getByRole('textbox');
    fireEvent.change(targetInput, { target: { value: 'archive/notes-2026.md' } });

    // Submit approve + execute (2 selected: op_2 and op_3)
    const runBtn = screen.getByRole('button', {
      name: tl('knowledge.organize.approveAndExecute', { count: 2 }),
    });
    fireEvent.click(runBtn);

    await waitFor(() => {
      expect(mocks.invoke).toHaveBeenCalledWith('cmd_kb_organize_approve', {
        kbId: 'kb_111111111111',
        planId: plan.id,
        decision: {
          reject: false,
          items: [
            { itemId: 'op_1', approved: false },
            { itemId: 'op_2', approved: true, overrideTargetRel: 'archive/notes-2026.md' },
            { itemId: 'op_3', approved: true },
          ],
        },
      });
    });

    expect(mocks.invoke).toHaveBeenCalledWith('cmd_kb_organize_execute', {
      kbId: 'kb_111111111111',
      planId: plan.id,
    });
    expect(onReload).toHaveBeenCalledTimes(1);
  });

  it('displays partial failure details and runs one-click undo via ConfirmDialog', async () => {
    const partialPlan: OrganizePlan = {
      ...makePendingPlan(),
      status: 'partially_completed',
      items: [
        {
          id: 'op_1',
          kind: 'rename',
          folderRoot: 'C:/docs/work',
          sourceRel: 'locked.md',
          targetRel: 'locked-renamed.md',
          reason: '重命名测试',
          affectedMarkdownLinks: 0,
          defaultSelected: true,
          approved: true,
          status: 'failed',
          error: 'FILE_IN_USE',
        },
        {
          id: 'op_2',
          kind: 'rename',
          folderRoot: 'C:/docs/work',
          sourceRel: 'free.md',
          targetRel: 'free-renamed.md',
          reason: '重命名自由文件',
          affectedMarkdownLinks: 0,
          defaultSelected: true,
          approved: true,
          status: 'succeeded',
        },
      ],
    };

    const onReload = vi.fn().mockResolvedValue(undefined);
    mocks.invoke.mockResolvedValue({
      planId: partialPlan.id,
      undone: ['op_2'],
      skippedModified: ['op_x'],
      conflicts: [],
      failed: [],
    });

    render(
      <ToastProvider>
        <KnowledgeOrganizeTab
          kb={makeKb()}
          plans={[partialPlan]}
          loading={false}
          onReload={onReload}
        />
      </ToastProvider>,
    );

    expect(screen.getByTestId('organize-plan-status')).toHaveTextContent(
      tl('knowledge.organize.status.partially_completed'),
    );
    expect(screen.getByTestId('organize-execution-summary')).toBeInTheDocument();
    expect(screen.getByRole('alert')).toHaveTextContent('FILE_IN_USE');

    // Click undo -> opens ConfirmDialog (never window.confirm)
    fireEvent.click(screen.getByRole('button', { name: tl('knowledge.organize.undoPlan') }));
    expect(screen.getByText(tl('knowledge.organize.undoTitle'))).toBeInTheDocument();
    const undoButtons = screen.getAllByRole('button', { name: tl('knowledge.organize.undoPlan') });
    fireEvent.click(undoButtons[undoButtons.length - 1]);

    await waitFor(() => {
      expect(mocks.invoke).toHaveBeenCalledWith('cmd_kb_organize_undo', {
        kbId: 'kb_111111111111',
        planId: partialPlan.id,
      });
    });

    expect(screen.getByTestId('organize-undo-report')).toBeInTheDocument();
    expect(onReload).toHaveBeenCalledTimes(1);
  });
});
