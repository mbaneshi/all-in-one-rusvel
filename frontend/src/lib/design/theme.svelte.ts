/**
 * Theme preference: system (OS), light, or dark. Cookie `rusvel-theme` persists choice.
 * Applies `light` / `dark` on documentElement for design tokens and Tailwind `dark:` variant.
 */

export type ThemePreference = 'system' | 'light' | 'dark';

const COOKIE = 'rusvel-theme';
const MAX_AGE_SEC = 31536000;

export let themePreference = $state<ThemePreference>('system');
export let systemPrefersDark = $state(false);

function resolvedFromState(): 'light' | 'dark' {
	if (themePreference === 'system') return systemPrefersDark ? 'dark' : 'light';
	return themePreference;
}

export function getResolvedTheme(): 'light' | 'dark' {
	return resolvedFromState();
}

function writeCookie(pref: ThemePreference) {
	if (typeof document === 'undefined') return;
	document.cookie = `${COOKIE}=${pref};path=/;max-age=${MAX_AGE_SEC};SameSite=Lax`;
}

export function applyThemeToDocument() {
	if (typeof document === 'undefined') return;
	const r = resolvedFromState();
	const root = document.documentElement;
	root.classList.toggle('light', r === 'light');
	root.classList.toggle('dark', r === 'dark');
}

function parsePreferenceFromCookie(): ThemePreference {
	if (typeof document === 'undefined') return 'system';
	const m = document.cookie.match(new RegExp(`(?:^|; )${COOKIE}=([^;]*)`));
	if (!m) return 'system';
	const v = decodeURIComponent(m[1].trim());
	if (v === 'system' || v === 'light' || v === 'dark') return v;
	return 'system';
}

export function setThemePreference(pref: ThemePreference) {
	themePreference = pref;
	writeCookie(pref);
	applyThemeToDocument();
}

/** Run once in root layout onMount; returns cleanup. */
export function initTheme(): () => void {
	if (typeof document === 'undefined' || typeof window === 'undefined') {
		return () => {};
	}
	themePreference = parsePreferenceFromCookie();
	const mql = window.matchMedia('(prefers-color-scheme: dark)');
	systemPrefersDark = mql.matches;
	applyThemeToDocument();

	const onChange = (e: MediaQueryListEvent) => {
		systemPrefersDark = e.matches;
		applyThemeToDocument();
	};
	mql.addEventListener('change', onChange);

	return () => mql.removeEventListener('change', onChange);
}
