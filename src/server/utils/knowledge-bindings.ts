import { isKnowledgeBaseId } from '../../shared/knowledge';
import { managementApi } from './management-api-client';

type BindingResult =
  | { ok: true; ids: string[] }
  | { ok: false; code: string; error: string; status: 400 | 404 | 503 };

export interface SessionKnowledgeLibrary {
  name: string;
  description?: string;
}

/** The Store owns existence checks; Session and Agent writers own their bindings. */
export async function validateKnowledgeBindings(value: unknown): Promise<BindingResult> {
  if (!Array.isArray(value) || !value.every(id => typeof id === 'string' && isKnowledgeBaseId(id))) {
    return {
      ok: false, code: 'KB_INVALID_INPUT', status: 400,
      error: 'knowledgeBaseIds must be a JSON array of library IDs',
    };
  }
  const ids = [...new Set<string>(value)];
  if (ids.length === 0) return { ok: true, ids };
  const result = await managementApi('/api/knowledge/validate-bindings', 'POST', { ids });
  if (result.ok === true) return { ok: true, ids };
  if (result.code === 'KB_NOT_FOUND') {
    return { ok: false, code: 'KB_NOT_FOUND', status: 404, error: 'One or more knowledge libraries do not exist' };
  }
  return { ok: false, code: 'KB_STORE_UNAVAILABLE', status: 503, error: 'Knowledge libraries are unavailable' };
}

export async function readSessionKnowledgeLibraries(
  ids?: readonly string[],
): Promise<SessionKnowledgeLibrary[]> {
  const sidecarId = process.env.MYAGENTS_SIDECAR_ID?.trim();
  if (!sidecarId || !ids?.length) return [];
  const response = await managementApi('/api/knowledge/list', 'POST', { sidecarId, request: {} });
  if (response.ok !== true || !Array.isArray(response.libraries)) return [];
  const bound = new Set(ids);
  return response.libraries.flatMap((entry: unknown) => {
    if (!entry || typeof entry !== 'object') return [];
    const { id, name, description } = entry as Record<string, unknown>;
    return typeof id === 'string' && bound.has(id) && typeof name === 'string'
      ? [{ name, ...(typeof description === 'string' ? { description } : {}) }]
      : [];
  });
}
