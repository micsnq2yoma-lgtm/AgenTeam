import atmerManifest from '@/assets/floating-pets/atmer/pet.json';
import atmerSpritesheetUrl from '@/assets/floating-pets/atmer/spritesheet.webp';

import { normalizePetManifest, type PetPack } from './petAtlas';

/** The AgenTeam logo as a 3D pixel pet: the coral lead and two linked teammates. */
export const DEFAULT_PET_PACK_ID = 'atmer';

function createBuiltinPetPack(manifestInput: unknown, spritesheetUrl: string): PetPack {
    const manifest = normalizePetManifest(manifestInput);
    if (!manifest) {
        throw new Error('[fb-pet] bundled pet manifest is invalid');
    }
    return {
        id: manifest.id,
        displayName: manifest.displayName,
        description: manifest.description,
        source: 'builtin',
        spritesheetUrl,
        atlas: manifest.atlas,
    };
}

export function normalizeBuiltinPetPackId(id: string | null | undefined): string | null {
    return id || null;
}

export const ATMER_PET_PACK = createBuiltinPetPack(atmerManifest, atmerSpritesheetUrl);

export const BUILTIN_PET_PACKS = [ATMER_PET_PACK] as const;

export const BUILTIN_PET_PACK_IDS = BUILTIN_PET_PACKS.map((pack) => pack.id);
