/**
 * NewAgentPanel — the single "New Agent Team" surface (list → detail in one dialog).
 *
 * Every "add workspace" entry opens this one instance. The list offers one
 * "New team" entry plus the user's own templates. A team is one folder: the
 * user picks its location (default / new folder / existing folder) and whether
 * assistant mode is on; `cmd_prepare_team_folder` prepares the folder for every
 * combination, so there is one creation path instead of one per card. Workspace registration always goes through
 * `ConfigProvider.addProject` with the create-only policy, so an already
 * registered folder is reported instead of silently re-labelled.
 */

import { memo, useCallback, useEffect, useLayoutEffect, useMemo, useRef, useState, type ReactNode } from 'react';
import { useTranslation } from 'react-i18next';
import type { TFunction } from 'i18next';
import { invoke } from '@tauri-apps/api/core';
import { basename, homeDir, join } from '@tauri-apps/api/path';
import { open as openDialog } from '@tauri-apps/plugin-dialog';
import { exists } from '@tauri-apps/plugin-fs';

import {
  BrainIcon,
  ChevronLeftIcon,
  ChevronRightIcon,
  CloseIcon,
  EditIcon,
  FileCheckIcon,
  FolderIcon,
  LoaderIcon,
  MessageCircleIcon,
  MoreIcon,
  PackageIcon,
  PlusIcon,
  TeamIcon,
  TerminalIcon,
  TrashIcon,
  WrenchIcon,
  FolderOpenIcon,
} from '@/components/icons';
import { track } from '@/analytics';
import ConfirmDialog from '@/components/ConfirmDialog';
import OverlayBackdrop from '@/components/OverlayBackdrop';
import PathInputDialog from '@/components/PathInputDialog';
import { useToast } from '@/components/Toast';
import { MenuItem } from '@/components/ui/MenuItem';
import { Popover } from '@/components/ui/Popover';
import type { Project, WorkspaceTemplate } from '@/config/types';
import { DEFAULT_BUNDLED_WORKSPACE_TEMPLATE_ID, PRESET_TEMPLATES } from '@/config/types';
import { ProjectAlreadyExistsError } from '@/config/services/projectService';
import { addUserTemplate, loadUserTemplates, removeUserTemplate, updateUserTemplate } from '@/config/services/templateService';
import { useCloseLayer } from '@/hooks/useCloseLayer';
import { useConfig } from '@/hooks/useConfig';
import { useWorkspaceFileService } from '@/hooks/useWorkspaceFileService';
import { isBrowserDevMode, pickFolderForDialog } from '@/utils/browserMock';
import { isImeComposingEvent } from '@/utils/imeKeyboard';
import { shortenPathForDisplay } from '@/utils/pathDetection';
import WorkspaceIcon from './WorkspaceIcon';
import WorkspaceIconGrid, { WORKSPACE_ICON_GRID_PANEL_CLASS } from './WorkspaceIconGrid';
import {
  classifyFolderConflict,
  deriveWorkspaceFolderName,
  detectProjectInstructionFile,
  detectTemplateContents,
  displaySeparator,
  PROJECT_INSTRUCTION_FILES,
  splitPathForDisplay,
  TEMPLATE_CONTENT_ENTRIES,
  type CheckedPaths,
} from './newAgentPanelModel';

// Above the sidebar flyout (z-240/245) and below Popover (260) / ConfirmDialog (300).
const PANEL_Z = 250;

type TeamLocation = 'default' | 'new' | 'existing';

interface TeamDraft {
  kind: 'team';
  location: TeamLocation;
  /** Chosen folder when `location === 'existing'`. */
  folderPath: string;
  /** Parent picked for `location === 'new'`. */
  parentDir: string;
  defaultParentDir: string;
  name: string;
  icon: string;
  assistant: boolean;
}

type Draft =
  | TeamDraft
  | { kind: 'template'; templateId: string; name: string; icon: string; parentDir: string };

type Page = { view: 'list' } | { view: 'detail'; draft: Draft };

type Busy = 'picking' | 'creating' | 'adding-template' | 'saving-template' | null;

interface TemplateEdit {
  name: string;
  description: string;
  icon: string;
}

interface NewAgentPanelProps {
  purpose?: Project['purpose'];
  onClose: () => void;
  /** Fired after the workspace is registered, right before the panel closes. */
  onCreated: (project: Project) => void;
}

function errorMessage(err: unknown): string {
  return err instanceof Error ? err.message : String(err);
}

/** Destination for a template copy: `<dir>/<name>`, then `<name>-2` … on collision. */
async function findAvailablePath(dir: string, name: string): Promise<string> {
  const first = await join(dir, name);
  if (!(await exists(first))) return first;
  for (let i = 2; i <= 100; i++) {
    const candidate = await join(dir, `${name}-${i}`);
    if (!(await exists(candidate))) return candidate;
  }
  return join(dir, `${name}-${Date.now()}`);
}

async function resolveDefaultParentDir(): Promise<string> {
  try {
    return await join(await homeDir(), '.myagents', 'projects');
  } catch {
    return '';
  }
}

const ASSISTANT_TEMPLATE = PRESET_TEMPLATES.find((tpl) => tpl.id === DEFAULT_BUNDLED_WORKSPACE_TEMPLATE_ID);

/** Parent directory a new team folder is created under (null when adopting an existing folder). */
function teamParentDir(draft: TeamDraft): string | null {
  if (draft.location === 'existing') return null;
  return draft.location === 'new' ? draft.parentDir : draft.defaultParentDir;
}

async function folderLeaf(path: string): Promise<string> {
  try {
    const name = await basename(path);
    if (name.trim()) return name;
  } catch {
    // fall through to the pure split
  }
  return splitPathForDisplay(path).leaf || path;
}

export default memo(function NewAgentPanel({ onClose, onCreated, purpose }: NewAgentPanelProps) {
  const { t } = useTranslation('launcher');
  const toast = useToast();
  const { projects, addProject } = useConfig();

  const [page, setPage] = useState<Page>({ view: 'list' });
  const [pageEnter, setPageEnter] = useState<'forward' | 'back' | null>(null);
  const [userTemplates, setUserTemplates] = useState<WorkspaceTemplate[]>([]);
  const [busy, setBusy] = useState<Busy>(null);
  const [error, setError] = useState<string | null>(null);
  const [iconPickerOpen, setIconPickerOpen] = useState(false);
  const [moreMenuOpen, setMoreMenuOpen] = useState(false);
  const [templateEdit, setTemplateEdit] = useState<TemplateEdit | null>(null);
  const [templateIconPickerOpen, setTemplateIconPickerOpen] = useState(false);
  const [templateToDelete, setTemplateToDelete] = useState<WorkspaceTemplate | null>(null);
  const [pathDialog, setPathDialog] = useState<{ folderName: string; defaultPath: string } | null>(null);
  const [detection, setDetection] = useState<{ root: string; results: CheckedPaths } | null>(null);
  const [resolvedFolder, setResolvedFolder] = useState<{ parentDir: string; folderName: string; leaf: string } | null>(null);

  const avatarRef = useRef<HTMLButtonElement>(null);
  const moreRef = useRef<HTMLButtonElement>(null);
  const templateIconRef = useRef<HTMLButtonElement>(null);
  const pathResolverRef = useRef<((path: string | null) => void) | null>(null);
  const mountedRef = useRef(false);
  const projectsRef = useRef(projects);
  const toastRef = useRef(toast);
  useLayoutEffect(() => {
    projectsRef.current = projects;
    toastRef.current = toast;
  }, [projects, toast]);

  useEffect(() => {
    mountedRef.current = true;
    return () => {
      mountedRef.current = false;
    };
  }, []);

  useEffect(() => {
    let cancelled = false;
    void loadUserTemplates()
      .then((templates) => {
        if (!cancelled) setUserTemplates(templates);
      })
      .catch((err: unknown) => console.warn('[NewAgentPanel] Failed to load templates:', err));
    return () => {
      cancelled = true;
    };
  }, []);

  const templates = useMemo(() => [...PRESET_TEMPLATES, ...userTemplates], [userTemplates]);
  const draft = page.view === 'detail' ? page.draft : null;
  const template = draft?.kind === 'template' ? templates.find((item) => item.id === draft.templateId) ?? null : null;

  // ---------- navigation ----------

  const resetDetailLayers = useCallback(() => {
    setIconPickerOpen(false);
    setMoreMenuOpen(false);
    setTemplateEdit(null);
    setTemplateIconPickerOpen(false);
    setError(null);
  }, []);

  const openDetail = useCallback((next: Draft) => {
    resetDetailLayers();
    setPageEnter('forward');
    setPage({ view: 'detail', draft: next });
  }, [resetDetailLayers]);

  const goBack = useCallback(() => {
    resetDetailLayers();
    setPageEnter('back');
    setPage({ view: 'list' });
  }, [resetDetailLayers]);

  const updateDraft = useCallback((patch: Partial<Draft>) => {
    setPage((current) => (current.view === 'detail'
      ? { view: 'detail', draft: { ...current.draft, ...patch } as Draft }
      : current));
  }, []);

  const requestClose = useCallback(() => {
    if (busy === 'creating') return;
    onClose();
  }, [busy, onClose]);

  useCloseLayer(() => {
    requestClose();
    return true;
  }, PANEL_Z);

  // ---------- folder picking ----------

  const pickFolder = useCallback(async (title: string): Promise<string | null> => {
    if (isBrowserDevMode()) {
      const info = await pickFolderForDialog();
      if (!info) return null;
      return new Promise((resolve) => {
        pathResolverRef.current = resolve;
        setPathDialog(info);
      });
    }
    const selected = await openDialog({ directory: true, multiple: false, title });
    return typeof selected === 'string' ? selected : null;
  }, []);

  const settlePathDialog = useCallback((path: string | null) => {
    setPathDialog(null);
    const resolve = pathResolverRef.current;
    pathResolverRef.current = null;
    resolve?.(path);
  }, []);

  /** Pick a local folder; a registered folder is rejected and the choice is discarded. */
  const pickLocalFolder = useCallback(async (): Promise<string | null> => {
    const path = await pickFolder(t('dialogs.pickProjectFolder'));
    if (!path || !mountedRef.current) return null;
    const conflict = classifyFolderConflict(projectsRef.current, path);
    if (conflict) {
      toastRef.current.error(conflict.kind === 'archived' ? t('newAgentPanel.errors.archived') : t('newAgentPanel.errors.exists'));
      return null;
    }
    return path;
  }, [pickFolder, t]);

  const handleOpenTeam = useCallback(async () => {
    if (busy) return;
    const defaultParentDir = await resolveDefaultParentDir();
    if (!mountedRef.current) return;
    openDetail({
      kind: 'team',
      location: 'default',
      folderPath: '',
      parentDir: defaultParentDir,
      defaultParentDir,
      name: '',
      icon: '',
      assistant: true,
    });
  }, [busy, openDetail]);

  /** Switch (or re-pick) the team location; a cancelled picker keeps the previous choice. */
  const handleSelectLocation = useCallback(async (location: TeamLocation) => {
    if (busy || draft?.kind !== 'team') return;
    if (location === 'default') {
      updateDraft({ location });
      return;
    }
    setBusy('picking');
    try {
      if (location === 'existing') {
        const path = await pickLocalFolder();
        if (!path || !mountedRef.current) return;
        const leaf = await folderLeaf(path);
        setPage((current) => {
          if (current.view !== 'detail' || current.draft.kind !== 'team') return current;
          // Keep a name the user typed; otherwise follow the folder.
          const name = current.draft.name.trim() ? current.draft.name : leaf;
          return { view: 'detail', draft: { ...current.draft, location, folderPath: path, name } };
        });
      } else {
        const parent = await pickFolder(t('newAgentPanel.pickParentDirTitle'));
        if (!parent || !mountedRef.current) return;
        updateDraft({ location, parentDir: parent });
      }
    } catch (err) {
      toastRef.current.error(t('newAgentPanel.errors.pickFailed', { message: errorMessage(err) }));
    } finally {
      if (mountedRef.current) setBusy(null);
    }
  }, [busy, draft, pickFolder, pickLocalFolder, t, updateDraft]);

  const handleOpenTemplate = useCallback(async (tpl: WorkspaceTemplate) => {
    if (busy) return;
    const parentDir = await resolveDefaultParentDir();
    if (!mountedRef.current) return;
    openDetail({
      kind: 'template',
      templateId: tpl.id,
      name: tpl.isBuiltin ? tpl.name.toLowerCase() : tpl.name,
      icon: tpl.icon ?? '',
      parentDir,
    });
  }, [busy, openDetail]);

  const handleChangeParentDir = useCallback(async () => {
    if (busy || isBrowserDevMode()) return;
    try {
      const selected = await openDialog({ directory: true, multiple: false, title: t('newAgentPanel.pickParentDirTitle') });
      if (typeof selected === 'string' && mountedRef.current) updateDraft({ parentDir: selected });
    } catch (err) {
      toastRef.current.error(t('newAgentPanel.errors.pickFailed', { message: errorMessage(err) }));
    }
  }, [busy, t, updateDraft]);

  // ---------- detection & path preview ----------

  const adoptingFolder = draft?.kind === 'team' && draft.location === 'existing' && !!draft.folderPath;
  const detectionRoot = adoptingFolder && draft?.kind === 'team'
    ? draft.folderPath
    : template && !template.isBuiltin && template.path ? template.path : null;
  const detectionKind = adoptingFolder ? 'local' : draft?.kind === 'template' ? 'template' : null;
  const fileService = useWorkspaceFileService(detectionRoot);
  const { checkPaths, openPathExternal } = fileService;

  useEffect(() => {
    if (!detectionRoot || !detectionKind) return;
    let cancelled = false;
    const paths = detectionKind === 'local'
      ? [...PROJECT_INSTRUCTION_FILES]
      : TEMPLATE_CONTENT_ENTRIES.map((entry) => entry.path);
    // Detection is advisory: any failure renders as "nothing detected".
    checkPaths({ paths })
      .then((result) => {
        if (!cancelled) setDetection({ root: detectionRoot, results: result?.results ?? {} });
      })
      .catch(() => {
        if (!cancelled) setDetection({ root: detectionRoot, results: {} });
      });
    return () => {
      cancelled = true;
    };
  }, [checkPaths, detectionKind, detectionRoot]);

  const detected = detection && detection.root === detectionRoot ? detection.results : null;

  const templateParentDir = draft?.kind === 'template'
    ? draft.parentDir
    : draft?.kind === 'team' ? teamParentDir(draft) ?? '' : '';
  const templateFolderName = templateParentDir && draft ? deriveWorkspaceFolderName(draft.name) : '';

  useEffect(() => {
    if (!templateParentDir || !templateFolderName) return;
    let cancelled = false;
    const timer = setTimeout(() => {
      findAvailablePath(templateParentDir, templateFolderName)
        .then(async (path) => {
          const leaf = await folderLeaf(path);
          if (!cancelled) setResolvedFolder({ parentDir: templateParentDir, folderName: templateFolderName, leaf });
        })
        .catch(() => {
          // Preview falls back to the derived name; creation re-resolves the path.
        });
    }, 250);
    return () => {
      cancelled = true;
      clearTimeout(timer);
    };
  }, [templateFolderName, templateParentDir]);

  // ---------- create ----------

  const canCreate = !!draft
    && draft.name.trim().length > 0
    && (draft.kind === 'team'
      ? (draft.location === 'existing' ? !!draft.folderPath : templateFolderName.length > 0 && templateParentDir.length > 0)
      : (templateFolderName.length > 0 && templateParentDir.length > 0 && !!template));

  const handleCreate = useCallback(async () => {
    if (!draft || busy || !canCreate) return;
    const displayName = draft.name.trim();
    setBusy('creating');
    setError(null);
    try {
      let project: Project;
      if (draft.kind === 'team') {
        const adopt = draft.location === 'existing';
        const parentDir = teamParentDir(draft);
        const destPath = adopt || !parentDir
          ? draft.folderPath
          : await findAvailablePath(parentDir, deriveWorkspaceFolderName(displayName));
        const assistant = draft.assistant && ASSISTANT_TEMPLATE ? ASSISTANT_TEMPLATE : null;
        // Adopting a folder without assistant mode needs no file work.
        if (!adopt || assistant) {
          await invoke('cmd_prepare_team_folder', {
            destPath,
            adoptExisting: adopt,
            assistantTemplateId: assistant?.id ?? null,
          });
        }
        project = await addProject(destPath, {
          purpose,
          displayName,
          icon: draft.icon || undefined,
          ...(assistant
            ? { templateId: assistant.id, templateSource: 'builtin' as const, agentDefaults: assistant.agentDefaults }
            : {}),
          onExisting: 'reject',
        });
        track('workspace_create', { source: 'team', location: draft.location, assistant: !!assistant });
        if (adopt && isBrowserDevMode()) {
          // Browser dev mode remembers the parent for its next path suggestion.
          const { parent } = splitPathForDisplay(draft.folderPath.replace(/\\/g, '/'));
          if (parent) window.localStorage.setItem('myagents:lastProjectDir', parent.replace(/\/$/, ''));
        }
      } else {
        if (!template) throw new Error('Template not found');
        const destPath = await findAvailablePath(draft.parentDir, deriveWorkspaceFolderName(displayName));
        if (template.isBuiltin) {
          await invoke('cmd_create_workspace_from_bundled_template', { templateId: template.id, destPath });
        } else if (template.path) {
          await invoke('cmd_create_workspace_from_template', { sourcePath: template.path, destPath });
        } else {
          throw new Error('Template has no source path');
        }
        project = await addProject(destPath, {
          purpose,
          displayName,
          icon: draft.icon || undefined,
          templateId: template.id,
          templateSource: template.isBuiltin ? 'builtin' : 'user',
          agentDefaults: template.isBuiltin ? template.agentDefaults : undefined,
          onExisting: 'reject',
        });
        track('workspace_create', { source: 'template', templateSource: template.isBuiltin ? 'builtin' : 'user' });
      }
      if (!mountedRef.current) return;
      setBusy(null);
      onCreated(project);
      onClose();
    } catch (err) {
      if (!mountedRef.current) return;
      setBusy(null);
      if (err instanceof ProjectAlreadyExistsError) {
        toastRef.current.error(err.archived ? t('newAgentPanel.errors.archived') : t('newAgentPanel.errors.exists'));
      } else {
        setError(t('newAgentPanel.errors.createFailed', { message: errorMessage(err) }));
      }
    }
  }, [addProject, busy, canCreate, draft, onClose, onCreated, purpose, t, template]);

  // ---------- user templates ----------

  const handleAddTemplate = useCallback(async () => {
    if (busy || isBrowserDevMode()) return;
    setBusy('adding-template');
    try {
      const selected = await openDialog({ directory: true, multiple: false, title: t('newAgentPanel.pickTemplateDirTitle') });
      if (typeof selected !== 'string') return;
      const folderName = await basename(selected);
      const destPath: string = await invoke('cmd_copy_folder_to_templates', { sourcePath: selected, templateName: folderName });
      const added = await addUserTemplate({ id: await basename(destPath), name: folderName, description: '', path: destPath });
      if (mountedRef.current) setUserTemplates((current) => [...current.filter((item) => item.id !== added.id), added]);
    } catch (err) {
      toastRef.current.error(t('newAgentPanel.errors.addTemplateFailed', { message: errorMessage(err) }));
    } finally {
      if (mountedRef.current) setBusy(null);
    }
  }, [busy, t]);

  const endTemplateEdit = useCallback(() => {
    setPageEnter('back');
    setTemplateEdit(null);
    setTemplateIconPickerOpen(false);
  }, []);

  const handleSaveTemplateEdit = useCallback(async () => {
    if (!template || template.isBuiltin || !templateEdit || busy) return;
    const name = templateEdit.name.trim();
    if (!name) return;
    const updates = { name, description: templateEdit.description.trim(), icon: templateEdit.icon };
    setBusy('saving-template');
    try {
      await updateUserTemplate(template.id, updates);
      if (!mountedRef.current) return;
      setUserTemplates((current) => current.map((item) => (item.id === template.id ? { ...item, ...updates } : item)));
      endTemplateEdit();
    } catch (err) {
      toastRef.current.error(t('newAgentPanel.errors.updateTemplateFailed', { message: errorMessage(err) }));
    } finally {
      if (mountedRef.current) setBusy(null);
    }
  }, [busy, endTemplateEdit, t, template, templateEdit]);

  const handleConfirmDeleteTemplate = useCallback(async () => {
    const target = templateToDelete;
    if (!target) return;
    setTemplateToDelete(null);
    try {
      // Folder first, then metadata — a failed folder delete keeps the template usable.
      if (target.path) await invoke('cmd_remove_template_folder', { templatePath: target.path });
      await removeUserTemplate(target.id);
      if (!mountedRef.current) return;
      setUserTemplates((current) => current.filter((item) => item.id !== target.id));
      goBack();
    } catch (err) {
      toastRef.current.error(t('newAgentPanel.errors.deleteTemplateFailed', { message: errorMessage(err) }));
    }
  }, [goBack, t, templateToDelete]);

  const handleRevealTemplate = useCallback(() => {
    setMoreMenuOpen(false);
    if (!template?.path) return;
    void openPathExternal({ fullPath: template.path }).catch((err: unknown) => {
      toastRef.current.error(errorMessage(err));
    });
  }, [openPathExternal, template]);

  const startTemplateEdit = useCallback(() => {
    if (!template || template.isBuiltin) return;
    setMoreMenuOpen(false);
    setPageEnter('forward');
    setTemplateEdit({ name: template.name, description: template.description ?? '', icon: template.icon ?? '' });
  }, [template]);

  // ---------- keyboard ----------

  // Document-level, like ConfirmDialog / Popover: closing a child layer (icon
  // popover, edit card, confirm) unmounts the focused element and drops focus
  // to <body>, where a dialog-scoped onKeyDown would never see Escape again.
  const handleEscapeKey = useCallback((event: KeyboardEvent) => {
    if (isImeComposingEvent(event) || event.key !== 'Escape') return;
    // Child layers (popovers, path / confirm dialogs) own their own Escape.
    if (pathDialog || templateToDelete || iconPickerOpen || moreMenuOpen || templateIconPickerOpen) return;
    event.preventDefault();
    if (busy === 'creating' || busy === 'saving-template') return;
    if (templateEdit) {
      endTemplateEdit();
      return;
    }
    if (page.view === 'detail') goBack();
    else onClose();
  }, [busy, endTemplateEdit, goBack, iconPickerOpen, moreMenuOpen, onClose, page.view, pathDialog, templateEdit, templateIconPickerOpen, templateToDelete]);

  // Child layers close themselves on the same keydown; the latest committed
  // handler still sees them open and yields, so one Escape closes one layer.
  const escapeHandlerRef = useRef(handleEscapeKey);
  useLayoutEffect(() => {
    escapeHandlerRef.current = handleEscapeKey;
  }, [handleEscapeKey]);
  useEffect(() => {
    const onKeyDown = (event: KeyboardEvent) => escapeHandlerRef.current(event);
    document.addEventListener('keydown', onKeyDown);
    return () => document.removeEventListener('keydown', onKeyDown);
  }, []);

  // ---------- render ----------

  const creating = busy === 'creating';
  const teamDraft = page.view === 'detail' && page.draft.kind === 'team' ? page.draft : null;
  const editingTemplate = templateEdit && template && !template.isBuiltin ? template : null;
  const pageKey = page.view === 'detail'
    ? `detail:${page.draft.kind}:${page.draft.kind === 'team' ? 'team' : page.draft.templateId}${editingTemplate ? ':edit' : ''}`
    : 'list';

  return (
    <OverlayBackdrop onClose={creating ? undefined : onClose} className="z-[250] px-4" portal>
      <div
        role="dialog"
        aria-modal="true"
        aria-label={t('newAgentPanel.title')}
        data-new-agent-panel
        className="flex max-h-[min(600px,calc(100vh-48px))] w-[540px] max-w-full flex-col overflow-hidden rounded-2xl border border-[var(--line)] bg-[var(--paper-elevated)] shadow-xl"
      >
        <div key={pageKey} className="new-agent-panel-page flex min-h-0 flex-1 flex-col" data-enter={pageEnter ?? undefined}>
          {page.view === 'list' ? (
            <ListPage
              userTemplates={userTemplates}
              busy={busy}
              onClose={onClose}
              onOpenTeam={handleOpenTeam}
              onOpenTemplate={handleOpenTemplate}
              onAddTemplate={handleAddTemplate}
            />
          ) : editingTemplate && templateEdit ? (
            <TemplateEditPage
              edit={templateEdit}
              saving={busy === 'saving-template'}
              iconRef={templateIconRef}
              iconPickerOpen={templateIconPickerOpen}
              onIconPickerOpenChange={setTemplateIconPickerOpen}
              onChange={setTemplateEdit}
              onSave={() => void handleSaveTemplateEdit()}
              onBack={endTemplateEdit}
              onClose={onClose}
            />
          ) : (
            <>
              {/* Header */}
              <div className="flex shrink-0 items-center gap-1 py-3 pl-3 pr-3">
                <button
                  type="button"
                  onClick={goBack}
                  disabled={creating}
                  aria-label={t('newAgentPanel.back')}
                  className="flex h-8 w-8 items-center justify-center rounded-lg text-[var(--ink-muted)] transition-colors hover:bg-[var(--paper-inset)] hover:text-[var(--ink)] disabled:opacity-50"
                >
                  <ChevronLeftIcon className="h-4 w-4" />
                </button>
                <h2 className="min-w-0 flex-1 truncate text-sm font-medium text-[var(--ink-muted)]">
                  {page.draft.kind === 'team'
                    ? t('newAgentPanel.team.detailTitle')
                    : template?.isBuiltin
                      ? builtinTitle(template, t)
                      : t('newAgentPanel.template.detailTitle', { name: template?.name ?? '' })}
                </h2>
                {template && !template.isBuiltin && (
                  <>
                    <button
                      ref={moreRef}
                      type="button"
                      onClick={() => setMoreMenuOpen((open) => !open)}
                      disabled={creating}
                      aria-label={t('newAgentPanel.template.more')}
                      className="flex h-8 w-8 items-center justify-center rounded-lg text-[var(--ink-muted)] transition-colors hover:bg-[var(--paper-inset)] hover:text-[var(--ink)] disabled:opacity-50"
                    >
                      <MoreIcon className="h-4 w-4" />
                    </button>
                    <Popover
                      open={moreMenuOpen}
                      onClose={() => setMoreMenuOpen(false)}
                      anchorRef={moreRef}
                      placement="bottom-end"
                      className="w-44 py-1"
                    >
                      <MenuItem icon={<EditIcon className="h-3.5 w-3.5" />} label={t('newAgentPanel.template.edit')} onClick={startTemplateEdit} />
                      <MenuItem icon={<FolderOpenIcon className="h-3.5 w-3.5" />} label={t('workspaceCard.openFolder')} onClick={handleRevealTemplate} />
                      <MenuItem
                        icon={<TrashIcon className="h-3.5 w-3.5" />}
                        label={t('newAgentPanel.template.delete')}
                        tone="danger"
                        onClick={() => {
                          setMoreMenuOpen(false);
                          setTemplateToDelete(template);
                        }}
                      />
                    </Popover>
                  </>
                )}
                <button
                  type="button"
                  onClick={onClose}
                  disabled={creating}
                  aria-label={t('newAgentPanel.close')}
                  className="flex h-8 w-8 items-center justify-center rounded-lg text-[var(--ink-muted)] transition-colors hover:bg-[var(--paper-inset)] hover:text-[var(--ink)] disabled:opacity-50"
                >
                  <CloseIcon className="h-4 w-4" />
                </button>
              </div>

              {/* Body */}
              <div className="min-h-0 flex-1 overflow-y-auto overscroll-contain px-6 pb-5 pt-1">
                {/* Identity */}
                <div className="flex items-center gap-4">
                  <button
                    ref={avatarRef}
                    type="button"
                    onClick={() => setIconPickerOpen((open) => !open)}
                    disabled={creating}
                    aria-label={t('newAgentPanel.changeIcon')}
                    title={t('newAgentPanel.changeIcon')}
                    className="relative flex h-[76px] w-[76px] shrink-0 items-center justify-center rounded-2xl border border-[var(--line)] bg-[var(--paper)] transition-colors hover:border-[var(--line-strong)]"
                  >
                    <WorkspaceIcon icon={page.draft.icon || undefined} size={44} />
                    <span className="absolute -bottom-1 -right-1 flex h-6 w-6 items-center justify-center rounded-full border border-[var(--line-strong)] bg-[var(--paper-elevated)] text-[var(--ink-muted)] shadow-sm">
                      <EditIcon className="h-3 w-3" />
                    </span>
                  </button>
                  <Popover
                    open={iconPickerOpen}
                    onClose={() => setIconPickerOpen(false)}
                    anchorRef={avatarRef}
                    placement="bottom-start"
                    offset={6}
                    unstyled
                    className={`rounded-xl border border-[var(--line)] bg-[var(--paper-elevated)] shadow-lg ${WORKSPACE_ICON_GRID_PANEL_CLASS}`}
                  >
                    <WorkspaceIconGrid
                      value={page.draft.icon || undefined}
                      onSelect={(icon) => {
                        updateDraft({ icon });
                        setIconPickerOpen(false);
                      }}
                    />
                  </Popover>

                  <div className="min-w-0 flex-1">
                    <label className="group/name relative flex h-[42px] cursor-text items-center rounded-[10px] border border-[var(--line-strong)] bg-[var(--paper)] transition-colors hover:border-[var(--ink-subtle)] focus-within:border-[var(--accent)] focus-within:bg-[var(--paper-elevated)] focus-within:ring-[3px] focus-within:ring-[var(--accent-warm-subtle)]">
                      <input
                        type="text"
                        autoFocus
                        value={page.draft.name}
                        disabled={creating}
                        onChange={(event) => updateDraft({ name: event.target.value })}
                        onKeyDown={(event) => {
                          if (isImeComposingEvent(event)) return;
                          if (event.key === 'Enter') {
                            event.preventDefault();
                            void handleCreate();
                          }
                        }}
                        placeholder={t('newAgentPanel.namePlaceholder')}
                        aria-label={t('newAgentPanel.nameLabel')}
                        spellCheck={false}
                        className="h-full w-full min-w-0 bg-transparent pl-3 pr-9 text-lg font-semibold text-[var(--ink)] outline-none placeholder:font-normal placeholder:text-[var(--ink-subtle)]"
                      />
                      <EditIcon className="pointer-events-none absolute right-3 h-3.5 w-3.5 text-[var(--ink-subtle)] transition-opacity group-focus-within/name:opacity-0" />
                    </label>
                    <PathLine
                      draft={page.draft}
                      previewLeaf={resolvedFolder
                        && resolvedFolder.parentDir === templateParentDir
                        && resolvedFolder.folderName === templateFolderName
                        ? resolvedFolder.leaf
                        : templateFolderName}
                      disabled={!!busy}
                      onChange={teamDraft
                        ? (teamDraft.location === 'default' ? undefined : () => void handleSelectLocation(teamDraft.location))
                        : handleChangeParentDir}
                    />
                  </div>
                </div>

                {teamDraft && (
                  <TeamOptions
                    draft={teamDraft}
                    disabled={!!busy}
                    onSelectLocation={(location) => void handleSelectLocation(location)}
                    onAssistantChange={(assistant) => updateDraft({ assistant })}
                  />
                )}

                <DetailIntro
                  draft={page.draft}
                  template={template}
                  detected={detected}
                  onAddDescription={startTemplateEdit}
                />
              </div>

              {/* Footer */}
              {error && (
                <p className="shrink-0 break-words px-6 pb-2 text-xs text-[var(--error)]" role="alert">{error}</p>
              )}
              <div className="flex shrink-0 items-center gap-3 border-t border-[var(--line)] px-6 py-4">
                <span className="min-w-0 flex-1 text-xs text-[var(--ink-subtle)]">
                  {teamDraft
                    ? (teamDraft.assistant ? t('newAgentPanel.mino.hint') : null)
                    : page.draft.kind === 'template' && template
                    ? template.isBuiltin
                      ? t('newAgentPanel.mino.hint')
                      : t('newAgentPanel.template.hint')
                    : null}
                </span>
                <button
                  type="button"
                  onClick={() => void handleCreate()}
                  disabled={!canCreate || !!busy}
                  className="flex shrink-0 items-center gap-1.5 rounded-full bg-[var(--button-primary-bg)] px-5 py-2.5 text-sm font-medium text-[var(--button-primary-text)] transition-colors hover:bg-[var(--button-primary-bg-hover)] disabled:opacity-50"
                >
                  {creating ? <LoaderIcon className="h-3.5 w-3.5 animate-spin" /> : <PlusIcon className="h-3.5 w-3.5" />}
                  {t('newAgentPanel.create')}
                </button>
              </div>
            </>
          )}
        </div>

        {/* Browser dev mode has no native picker; reuse the shared path input. */}
        <PathInputDialog
          isOpen={pathDialog !== null}
          folderName={pathDialog?.folderName ?? ''}
          defaultPath={pathDialog?.defaultPath ?? ''}
          onConfirm={(path) => settlePathDialog(path)}
          onCancel={() => settlePathDialog(null)}
        />

        {templateToDelete && (
          <ConfirmDialog
            title={t('newAgentPanel.template.deleteDialogTitle')}
            message={t('newAgentPanel.template.deleteDialogMessage', { name: templateToDelete.name })}
            confirmText={t('rightRail.delete')}
            cancelText={t('newAgentPanel.template.cancel')}
            confirmVariant="danger"
            onConfirm={() => void handleConfirmDeleteTemplate()}
            onCancel={() => setTemplateToDelete(null)}
          />
        )}
      </div>
    </OverlayBackdrop>
  );
});

type TFn = TFunction<'launcher'>;

function builtinTitle(tpl: WorkspaceTemplate, t: TFn): string {
  return tpl.id === DEFAULT_BUNDLED_WORKSPACE_TEMPLATE_ID ? t('newAgentPanel.mino.title') : tpl.name;
}

// ---------- list page ----------

function ListPage({
  userTemplates,
  busy,
  onClose,
  onOpenTeam,
  onOpenTemplate,
  onAddTemplate,
}: {
  userTemplates: WorkspaceTemplate[];
  busy: Busy;
  onClose: () => void;
  onOpenTeam: () => void;
  onOpenTemplate: (tpl: WorkspaceTemplate) => void;
  onAddTemplate: () => void;
}) {
  const { t } = useTranslation('launcher');
  const disabled = busy !== null;
  return (
    <>
      <div className="flex shrink-0 items-center justify-between py-4 pl-6 pr-4">
        <h2 className="text-lg font-semibold text-[var(--ink)]">{t('newAgentPanel.title')}</h2>
        <button
          type="button"
          onClick={onClose}
          aria-label={t('newAgentPanel.close')}
          className="flex h-8 w-8 items-center justify-center rounded-lg text-[var(--ink-muted)] transition-colors hover:bg-[var(--paper-inset)] hover:text-[var(--ink)]"
        >
          <CloseIcon className="h-4 w-4" />
        </button>
      </div>
      <div className="min-h-0 flex-1 overflow-y-auto overscroll-contain px-4 pb-4" data-new-agent-panel-list>
        <PanelRow
          autoFocus
          disabled={disabled}
          onClick={onOpenTeam}
          icon={(
            <span className="flex h-[42px] w-[42px] items-center justify-center rounded-xl bg-[var(--accent-warm-subtle)] text-[var(--accent)]">
              <TeamIcon className="h-5 w-5" />
            </span>
          )}
          title={t('newAgentPanel.team.title')}
          description={t('newAgentPanel.team.description')}
        />

        <div className="px-3 pb-1 pt-4 text-xs font-medium text-[var(--ink-subtle)]">{t('newAgentPanel.myTemplates')}</div>
        {userTemplates.map((tpl) => (
          <PanelRow
            key={tpl.id}
            disabled={disabled}
            onClick={() => onOpenTemplate(tpl)}
            icon={<TemplateGlyph icon={tpl.icon} />}
            title={tpl.name}
            description={tpl.description || <span className="text-[var(--ink-subtle)]">{t('newAgentPanel.noDescription')}</span>}
          />
        ))}
        <PanelRow
          disabled={disabled}
          busy={busy === 'adding-template'}
          onClick={onAddTemplate}
          trailing={false}
          muted
          icon={(
            <span className="flex h-[42px] w-[42px] items-center justify-center rounded-xl border-[1.5px] border-dashed border-[var(--line-strong)] text-[var(--ink-muted)]">
              <PlusIcon className="h-4 w-4" />
            </span>
          )}
          title={t('newAgentPanel.addTemplate')}
          description={t('newAgentPanel.addTemplateDescription')}
        />
      </div>
    </>
  );
}

function TemplateGlyph({ icon }: { icon?: string }) {
  return (
    <span className="flex h-[42px] w-[42px] items-center justify-center rounded-xl bg-[var(--paper-inset)]">
      <WorkspaceIcon icon={icon} size={24} />
    </span>
  );
}

function PanelRow({
  icon,
  title,
  badge,
  description,
  onClick,
  disabled,
  busy,
  trailing = true,
  muted,
  autoFocus,
}: {
  icon: ReactNode;
  title: string;
  badge?: string;
  description: ReactNode;
  onClick: () => void;
  disabled?: boolean;
  busy?: boolean;
  trailing?: boolean;
  muted?: boolean;
  autoFocus?: boolean;
}) {
  return (
    <button
      type="button"
      autoFocus={autoFocus}
      onClick={onClick}
      disabled={disabled}
      className="group/row flex w-full items-center gap-3.5 rounded-xl p-3 text-left transition-colors hover:bg-[var(--hover-bg)] focus-visible:bg-[var(--hover-bg)] focus-visible:outline-none disabled:cursor-default disabled:hover:bg-transparent"
    >
      {icon}
      <span className="min-w-0 flex-1">
        <span className={`flex items-center gap-1.5 text-sm font-semibold ${muted ? 'font-medium text-[var(--ink-muted)] group-hover/row:text-[var(--ink)]' : 'text-[var(--ink)]'}`}>
          <span className="truncate">{title}</span>
          {badge && (
            <span className="shrink-0 rounded-full bg-[var(--accent-warm-subtle)] px-1.5 py-px text-xs font-medium text-[var(--accent)]">{badge}</span>
          )}
        </span>
        <span className="mt-0.5 block truncate text-xs text-[var(--ink-muted)]">{description}</span>
      </span>
      {busy
        ? <LoaderIcon className="h-4 w-4 shrink-0 animate-spin text-[var(--ink-subtle)]" />
        : trailing && <ChevronRightIcon className="h-4 w-4 shrink-0 text-[var(--ink-subtle)]" />}
    </button>
  );
}

// ---------- detail pieces ----------

function PathLine({
  draft,
  previewLeaf,
  disabled,
  onChange,
}: {
  draft: Draft;
  previewLeaf: string;
  disabled: boolean;
  /** Omitted when the location has nothing to change (the default location). */
  onChange?: () => void;
}) {
  const { t } = useTranslation('launcher');
  let parent: string;
  let leaf: string;
  const fixedFolder = draft.kind === 'team' && draft.location === 'existing' ? draft.folderPath : null;
  const parentDir = draft.kind === 'team' ? teamParentDir(draft) ?? '' : draft.parentDir;
  if (fixedFolder !== null) {
    ({ parent, leaf } = splitPathForDisplay(shortenPathForDisplay(fixedFolder)));
  } else {
    const shortParent = shortenPathForDisplay(parentDir);
    parent = shortParent ? `${shortParent.replace(/[\\/]+$/, '')}${displaySeparator(shortParent)}` : '';
    leaf = previewLeaf || '…';
  }
  return (
    <div className="mt-2 flex min-w-0 items-center gap-1.5 pl-0.5 text-sm text-[var(--ink-muted)]" data-new-agent-panel-path>
      <FolderIcon className="h-3.5 w-3.5 shrink-0 text-[var(--ink-subtle)]" />
      <span className="min-w-0 flex-1 truncate" title={fixedFolder ?? `${parentDir}${displaySeparator(parentDir)}${leaf}`}>
        {parent}
        <span className="font-medium text-[var(--ink)]">{leaf}</span>
      </span>
      {onChange && (
        <button
          type="button"
          onClick={onChange}
          disabled={disabled}
          className="shrink-0 rounded-md bg-[var(--button-secondary-bg)] px-2.5 py-1 text-xs font-medium text-[var(--button-secondary-text)] transition-colors hover:bg-[var(--button-secondary-bg-hover)] disabled:opacity-50"
        >
          {t('newAgentPanel.changePath')}
        </button>
      )}
    </div>
  );
}

const TEAM_LOCATIONS: readonly TeamLocation[] = ['default', 'new', 'existing'];
const TEAM_LOCATION_LABEL_KEYS = {
  default: 'newAgentPanel.team.locationDefault',
  new: 'newAgentPanel.team.locationNew',
  existing: 'newAgentPanel.team.locationExisting',
} as const;

function TeamOptions({
  draft,
  disabled,
  onSelectLocation,
  onAssistantChange,
}: {
  draft: TeamDraft;
  disabled: boolean;
  onSelectLocation: (location: TeamLocation) => void;
  onAssistantChange: (assistant: boolean) => void;
}) {
  const { t } = useTranslation('launcher');
  return (
    <div className="mt-5 flex flex-col gap-4" data-new-agent-panel-team-options>
      <div>
        <div className="mb-2 text-xs font-medium text-[var(--ink-muted)]" id="new-team-location-label">{t('newAgentPanel.team.locationLabel')}</div>
        <div role="radiogroup" aria-labelledby="new-team-location-label" className="grid grid-cols-3 gap-1 rounded-[10px] bg-[var(--paper-inset)] p-1">
          {TEAM_LOCATIONS.map((location) => {
            const selected = draft.location === location;
            return (
              <button
                key={location}
                type="button"
                role="radio"
                aria-checked={selected}
                disabled={disabled}
                onClick={() => onSelectLocation(location)}
                className={`h-8 rounded-[8px] px-2 text-sm transition-colors focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-[var(--accent)] disabled:opacity-60 ${
                  selected
                    ? 'bg-[var(--paper-elevated)] font-medium text-[var(--ink)] shadow-xs'
                    : 'text-[var(--ink-muted)] hover:text-[var(--ink)]'
                }`}
              >
                {t(TEAM_LOCATION_LABEL_KEYS[location])}
              </button>
            );
          })}
        </div>
      </div>
      <div className="flex items-center gap-3 rounded-xl border border-[var(--line)] bg-[var(--paper)] px-3.5 py-3">
        <span className="flex h-7 w-7 shrink-0 items-center justify-center rounded-lg bg-[var(--accent-warm-subtle)] text-[var(--accent)]">
          <BrainIcon className="h-4 w-4" />
        </span>
        <div className="min-w-0 flex-1">
          <div className="text-sm font-medium text-[var(--ink)]" id="new-team-assistant-label">{t('newAgentPanel.team.assistantTitle')}</div>
          <div className="text-xs text-[var(--ink-muted)]">{t('newAgentPanel.team.assistantDescription')}</div>
        </div>
        <button
          type="button"
          role="switch"
          aria-checked={draft.assistant}
          aria-labelledby="new-team-assistant-label"
          disabled={disabled}
          onClick={() => onAssistantChange(!draft.assistant)}
          className={`relative h-6 w-11 shrink-0 cursor-pointer rounded-full transition-colors focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-[var(--accent)] disabled:opacity-60 ${
            draft.assistant ? 'bg-[var(--accent)]' : 'bg-[var(--line-strong)]'
          }`}
        >
          <span
            className={`absolute left-0.5 top-0.5 h-5 w-5 rounded-full bg-[var(--toggle-thumb)] shadow transition-transform ${
              draft.assistant ? 'translate-x-5' : 'translate-x-0'
            }`}
          />
        </button>
      </div>
    </div>
  );
}

interface Capability {
  key: string;
  icon: ReactNode;
  title: ReactNode;
  description: ReactNode;
  extra?: ReactNode;
  placeholder?: boolean;
}

function DetailIntro({
  draft,
  template,
  detected,
  onAddDescription,
}: {
  draft: Draft;
  template: WorkspaceTemplate | null;
  detected: CheckedPaths | null;
  onAddDescription: () => void;
}) {
  const { t } = useTranslation('launcher');
  const iconClass = 'h-4 w-4';
  const channels: Capability = {
    key: 'channels',
    icon: <MessageCircleIcon className={iconClass} />,
    title: t('newAgentPanel.capChannelsTitle'),
    description: t('newAgentPanel.capChannelsDescription'),
  };

  let intro: ReactNode;
  let capabilities: Capability[];
  if (draft.kind === 'team' && draft.assistant) {
    intro = t('newAgentPanel.team.assistantIntro');
    capabilities = [
      {
        key: 'evolve',
        icon: <BrainIcon className={iconClass} />,
        title: t('newAgentPanel.mino.capEvolveTitle'),
        description: t('newAgentPanel.mino.capEvolveDescription'),
      },
      {
        key: 'skills',
        icon: <WrenchIcon className={iconClass} />,
        title: t('newAgentPanel.mino.capSkillsTitle'),
        description: t('newAgentPanel.mino.capSkillsDescription'),
      },
      ...(draft.location === 'existing'
        ? [{
          key: 'keep-files',
          icon: <FileCheckIcon className={iconClass} />,
          title: t('newAgentPanel.team.capKeepFilesTitle'),
          description: t('newAgentPanel.team.capKeepFilesDescription'),
        }]
        : []),
      channels,
    ];
  } else if (draft.kind === 'team') {
    const adopting = draft.location === 'existing' && !!draft.folderPath;
    const instructionFile = adopting && detected ? detectProjectInstructionFile(detected) : null;
    intro = t('newAgentPanel.team.plainIntro');
    capabilities = [
      {
        key: 'files',
        icon: <TerminalIcon className={iconClass} />,
        title: t('newAgentPanel.local.capFilesTitle'),
        description: t('newAgentPanel.local.capFilesDescription'),
      },
      ...(!adopting ? [] : [detected === null
        ? { key: 'instructions', icon: <FileCheckIcon className={iconClass} />, title: ' ', description: ' ', placeholder: true }
        : instructionFile
          ? {
            key: 'instructions',
            icon: <FileCheckIcon className={iconClass} />,
            title: <span className="text-[var(--success)]">{t('newAgentPanel.local.capInstructionsFoundTitle', { file: instructionFile })}</span>,
            description: t('newAgentPanel.local.capInstructionsFoundDescription'),
          }
          : {
            key: 'instructions',
            icon: <FileCheckIcon className={iconClass} />,
            title: t('newAgentPanel.local.capInstructionsMissingTitle'),
            description: t('newAgentPanel.local.capInstructionsMissingDescription'),
          }]),
      channels,
    ];
  } else if (template?.isBuiltin) {
    const isMino = template.id === DEFAULT_BUNDLED_WORKSPACE_TEMPLATE_ID;
    intro = isMino ? t('newAgentPanel.mino.intro') : template.description;
    capabilities = isMino
      ? [
        {
          key: 'evolve',
          icon: <BrainIcon className={iconClass} />,
          title: t('newAgentPanel.mino.capEvolveTitle'),
          description: t('newAgentPanel.mino.capEvolveDescription'),
        },
        {
          key: 'skills',
          icon: <WrenchIcon className={iconClass} />,
          title: t('newAgentPanel.mino.capSkillsTitle'),
          description: t('newAgentPanel.mino.capSkillsDescription'),
        },
        channels,
      ]
      : [channels];
  } else {
    intro = template?.description
      ? template.description
      : (
        <>
          <span className="text-[var(--ink-subtle)]">{t('newAgentPanel.template.noDescription')}</span>{' '}
          <button type="button" onClick={onAddDescription} className="text-[var(--ink-muted)] underline underline-offset-2 hover:text-[var(--ink)]">
            {t('newAgentPanel.template.addDescription')}
          </button>
        </>
      );
    const contents = detected ? detectTemplateContents(detected) : [];
    capabilities = [{
      key: 'contents',
      icon: <PackageIcon className={iconClass} />,
      title: t('newAgentPanel.template.capContentsTitle'),
      description: t('newAgentPanel.template.capContentsDescription'),
      extra: contents.length > 0 ? (
        <div className="mt-2 flex flex-wrap gap-1.5">
          {contents.map((label) => (
            <span key={label} className="rounded-md bg-[var(--paper-inset)] px-2 py-0.5 font-mono text-xs text-[var(--ink-muted)]">{label}</span>
          ))}
        </div>
      ) : null,
    }];
  }

  return (
    <>
      <p className="mt-5 text-sm leading-relaxed text-[var(--ink-muted)]">{intro}</p>
      <div className="mt-5">
        <div className="mb-2 text-xs font-medium text-[var(--ink-muted)]">{t('newAgentPanel.capabilitiesTitle')}</div>
        <ul className="divide-y divide-[var(--line)] rounded-xl border border-[var(--line)] bg-[var(--paper)]" data-new-agent-panel-capabilities>
          {capabilities.map((cap) => (
            <li key={cap.key} className="flex items-start gap-3 px-3.5 py-3" aria-hidden={cap.placeholder || undefined}>
              <span className="flex h-7 w-7 shrink-0 items-center justify-center rounded-lg bg-[var(--paper-inset)] text-[var(--ink-muted)]">{cap.icon}</span>
              <div className="min-w-0 flex-1">
                <div className="text-sm font-medium text-[var(--ink)]">{cap.title}</div>
                <div className="text-xs text-[var(--ink-muted)]">{cap.description}</div>
                {cap.extra}
              </div>
            </li>
          ))}
        </ul>
      </div>
    </>
  );
}

/** Third page of the dialog: edits the user template itself, not the Agent being created. */
function TemplateEditPage({
  edit,
  saving,
  iconRef,
  iconPickerOpen,
  onIconPickerOpenChange,
  onChange,
  onSave,
  onBack,
  onClose,
}: {
  edit: TemplateEdit;
  saving: boolean;
  iconRef: React.RefObject<HTMLButtonElement | null>;
  iconPickerOpen: boolean;
  onIconPickerOpenChange: (open: boolean) => void;
  onChange: (edit: TemplateEdit) => void;
  onSave: () => void;
  onBack: () => void;
  onClose: () => void;
}) {
  const { t } = useTranslation('launcher');
  const canSave = edit.name.trim().length > 0 && !saving;
  const headerButtonClass = 'flex h-8 w-8 items-center justify-center rounded-lg text-[var(--ink-muted)] transition-colors hover:bg-[var(--paper-inset)] hover:text-[var(--ink)] disabled:opacity-50';
  return (
    <>
      <div className="flex shrink-0 items-center gap-1 py-3 pl-3 pr-3">
        <button type="button" onClick={onBack} disabled={saving} aria-label={t('newAgentPanel.back')} className={headerButtonClass}>
          <ChevronLeftIcon className="h-4 w-4" />
        </button>
        <h2 className="min-w-0 flex-1 truncate text-sm font-medium text-[var(--ink-muted)]">{t('newAgentPanel.template.editTitle')}</h2>
        <button type="button" onClick={onClose} disabled={saving} aria-label={t('newAgentPanel.close')} className={headerButtonClass}>
          <CloseIcon className="h-4 w-4" />
        </button>
      </div>

      <div className="min-h-0 flex-1 overflow-y-auto overscroll-contain px-6 pb-5 pt-1" data-new-agent-panel-template-edit>
        <div className="flex items-center gap-4">
          <button
            ref={iconRef}
            type="button"
            onClick={() => onIconPickerOpenChange(!iconPickerOpen)}
            disabled={saving}
            aria-label={t('newAgentPanel.template.changeIcon')}
            title={t('newAgentPanel.template.changeIcon')}
            className="relative flex h-[76px] w-[76px] shrink-0 items-center justify-center rounded-2xl border border-[var(--line)] bg-[var(--paper)] transition-colors hover:border-[var(--line-strong)]"
          >
            <WorkspaceIcon icon={edit.icon || undefined} size={44} />
            <span className="absolute -bottom-1 -right-1 flex h-6 w-6 items-center justify-center rounded-full border border-[var(--line-strong)] bg-[var(--paper-elevated)] text-[var(--ink-muted)] shadow-sm">
              <EditIcon className="h-3 w-3" />
            </span>
          </button>
          <Popover
            open={iconPickerOpen}
            onClose={() => onIconPickerOpenChange(false)}
            anchorRef={iconRef}
            placement="bottom-start"
            offset={6}
            unstyled
            className={`rounded-xl border border-[var(--line)] bg-[var(--paper-elevated)] shadow-lg ${WORKSPACE_ICON_GRID_PANEL_CLASS}`}
          >
            <WorkspaceIconGrid
              value={edit.icon || undefined}
              onSelect={(icon) => {
                onChange({ ...edit, icon });
                onIconPickerOpenChange(false);
              }}
            />
          </Popover>
          <label className="min-w-0 flex-1">
            <span className="mb-1.5 block text-xs font-medium text-[var(--ink-muted)]">{t('newAgentPanel.template.nameLabel')}</span>
            <input
              type="text"
              autoFocus
              value={edit.name}
              disabled={saving}
              onChange={(event) => onChange({ ...edit, name: event.target.value })}
              onKeyDown={(event) => {
                if (isImeComposingEvent(event)) return;
                if (event.key === 'Enter') {
                  event.preventDefault();
                  if (canSave) onSave();
                }
              }}
              placeholder={t('newAgentPanel.template.namePlaceholder')}
              spellCheck={false}
              className="h-[42px] w-full rounded-[10px] border border-[var(--line-strong)] bg-[var(--paper)] px-3 text-base font-semibold text-[var(--ink)] outline-none transition-colors placeholder:font-normal placeholder:text-[var(--ink-subtle)] hover:border-[var(--ink-subtle)] focus:border-[var(--accent)] focus:bg-[var(--paper-elevated)] focus:ring-[3px] focus:ring-[var(--accent-warm-subtle)]"
            />
          </label>
        </div>

        <label className="mt-6 block">
          <span className="mb-1.5 block text-xs font-medium text-[var(--ink-muted)]">{t('newAgentPanel.template.descriptionLabel')}</span>
          <textarea
            value={edit.description}
            disabled={saving}
            onChange={(event) => onChange({ ...edit, description: event.target.value })}
            placeholder={t('newAgentPanel.template.descriptionPlaceholder')}
            rows={4}
            className="w-full resize-none rounded-[10px] border border-[var(--line-strong)] bg-[var(--paper)] px-3 py-2.5 text-sm leading-relaxed text-[var(--ink)] outline-none transition-colors placeholder:text-[var(--ink-subtle)] hover:border-[var(--ink-subtle)] focus:border-[var(--accent)] focus:bg-[var(--paper-elevated)] focus:ring-[3px] focus:ring-[var(--accent-warm-subtle)]"
          />
          <span className="mt-1.5 block text-xs text-[var(--ink-subtle)]">{t('newAgentPanel.template.descriptionHint')}</span>
        </label>
      </div>

      <div className="flex shrink-0 items-center gap-3 border-t border-[var(--line)] px-6 py-4">
        <span className="min-w-0 flex-1 text-xs text-[var(--ink-subtle)]">{t('newAgentPanel.template.editHint')}</span>
        <button
          type="button"
          onClick={onBack}
          disabled={saving}
          className="shrink-0 rounded-full px-4 py-2.5 text-sm font-medium text-[var(--ink-muted)] transition-colors hover:bg-[var(--hover-bg)] hover:text-[var(--ink)] disabled:opacity-50"
        >
          {t('newAgentPanel.template.cancel')}
        </button>
        <button
          type="button"
          onClick={onSave}
          disabled={!canSave}
          className="flex shrink-0 items-center gap-1.5 rounded-full bg-[var(--button-primary-bg)] px-5 py-2.5 text-sm font-medium text-[var(--button-primary-text)] transition-colors hover:bg-[var(--button-primary-bg-hover)] disabled:opacity-50"
        >
          {saving && <LoaderIcon className="h-3.5 w-3.5 animate-spin" />}
          {t('newAgentPanel.template.save')}
        </button>
      </div>
    </>
  );
}
