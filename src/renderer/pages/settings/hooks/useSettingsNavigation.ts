import { useCallback, useEffect, useRef, useState } from 'react';
import { isDeveloperSectionUnlocked } from '@/utils/developerMode';

import {
  VALID_SECTIONS,
  type SettingsSection,
} from '../settingsSections';

interface UseSettingsNavigationParams {
  initialSection?: string;
  /** Mount-only default; removing a consumed navigation target must preserve local selection. */
  defaultSection?: SettingsSection;
  navigationNonce?: number;
  onSectionChange?: () => void;
}

export function useSettingsNavigation({
  initialSection,
  defaultSection = 'providers',
  navigationNonce,
  onSectionChange,
}: UseSettingsNavigationParams) {
  const onSectionChangeRef = useRef(onSectionChange);

  useEffect(() => {
    onSectionChangeRef.current = onSectionChange;
  }, [onSectionChange]);

  const getInitialSection = (): SettingsSection => {
    if (initialSection && VALID_SECTIONS.includes(initialSection as SettingsSection)) {
      if (initialSection === 'developer' && !isDeveloperSectionUnlocked()) {
        return 'about';
      }
      return initialSection as SettingsSection;
    }
    return defaultSection;
  };

  const [activeSection, setActiveSection] = useState<SettingsSection>(getInitialSection);

  const notifySectionChange = useCallback(() => {
    onSectionChangeRef.current?.();
  }, []);

  useEffect(() => {
    if (initialSection && VALID_SECTIONS.includes(initialSection as SettingsSection)) {
      const timer = window.setTimeout(() => {
        if (initialSection === 'developer' && !isDeveloperSectionUnlocked()) {
          setActiveSection('about');
          notifySectionChange();
          return;
        }
        setActiveSection(initialSection as SettingsSection);
        notifySectionChange();
      }, 0);

      return () => {
        window.clearTimeout(timer);
      };
    }
  }, [initialSection, navigationNonce, notifySectionChange]);

  const navigateToProxySettings = useCallback(() => {
    setActiveSection('proxy');
  }, []);

  return {
    activeSection,
    setActiveSection,
    navigateToProxySettings,
    notifySectionChange,
  };
}
