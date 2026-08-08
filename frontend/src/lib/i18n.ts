import i18n from 'i18next';
import LanguageDetector from 'i18next-browser-languagedetector';
import { initReactI18next } from 'react-i18next';

// English translations files
import activityEn from '@/locales/en/activity.json';
import commonEn from '@/locales/en/common.json';
import navEn from '@/locales/en/nav.json';
import errorsEn from '@/locales/en/errors.json';
import landingEn from '@/locales/en/landing.json';
import authEn from '@/locales/en/auth.json';
import workspacesEn from '@/locales/en/workspaces.json';
import teamsEn from '@/locales/en/teams.json';
import challengesEn from '@/locales/en/challenges.json';
import writeupsEn from '@/locales/en/writeups.json';
import statsEn from '@/locales/en/stats.json';
import settingsEn from '@/locales/en/settings.json';
import adminEn from '@/locales/en/admin.json';

// French translations files
import activityFr from '@/locales/fr/activity.json';
import commonFr from '@/locales/fr/common.json';
import navFr from '@/locales/fr/nav.json';
import errorsFr from '@/locales/fr/errors.json';
import landingFr from '@/locales/fr/landing.json';
import authFr from '@/locales/fr/auth.json';
import workspacesFr from '@/locales/fr/workspaces.json';
import teamsFr from '@/locales/fr/teams.json';
import challengesFr from '@/locales/fr/challenges.json';
import writeupsFr from '@/locales/fr/writeups.json';
import statsFr from '@/locales/fr/stats.json';
import settingsFr from '@/locales/fr/settings.json';
import adminFr from '@/locales/fr/admin.json';

const resources = {
  en: {
    common: commonEn,
    nav: navEn,
    errors: errorsEn,
    landing: landingEn,
    auth: authEn,
    workspaces: workspacesEn,
    teams: teamsEn,
    challenges: challengesEn,
    writeups: writeupsEn,
    stats: statsEn,
    settings: settingsEn,
    admin: adminEn,
    activity: activityEn,
  },
  fr: {
    common: commonFr,
    nav: navFr,
    errors: errorsFr,
    landing: landingFr,
    auth: authFr,
    workspaces: workspacesFr,
    teams: teamsFr,
    challenges: challengesFr,
    writeups: writeupsFr,
    stats: statsFr,
    settings: settingsFr,
    admin: adminFr,
    activity: activityFr,
  },
};

const namespaces = Object.keys(resources.en);

i18n
  .use(LanguageDetector)
  .use(initReactI18next)
  .init({
    resources,
    defaultNS: 'common',
    ns: Array.from(namespaces),
    fallbackLng: 'en',
    supportedLngs: ['en', 'fr'],
    nonExplicitSupportedLngs: true,
    detection: {
      order: ['localStorage', 'navigator'],
      caches: ['localStorage'],
      lookupLocalStorage: 'pwnforge-lang',
    },
    interpolation: { escapeValue: false },
    saveMissing: import.meta.env.DEV,
    missingKeyHandler: (_lngs, ns, key) =>
      console.warn(`[i18n] Missing key : ${ns}:${key}`),
  });

export default i18n;
