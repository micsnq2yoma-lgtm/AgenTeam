import stylesheetText from './default-black.css?inline';
import type { PresetThemeManifest } from './preset-theme';

export const defaultBlackThemeManifest = {
  id: 'default-black',
  displayName: 'AgenTeam Classic2',
  description: 'AgenTeam Classic with neutral-black primary buttons',
  stylesheetText,
} satisfies PresetThemeManifest;
