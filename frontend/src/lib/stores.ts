import { writable } from 'svelte/store';
import { getPendingApprovals } from './api';
import type { SessionSummary, DepartmentDef } from './api';

/** Count of jobs in `AwaitingApproval`; drives sidebar badge. */
export const pendingApprovalCount = writable(0);

export async function refreshPendingApprovalCount(): Promise<void> {
	try {
		const jobs = await getPendingApprovals();
		pendingApprovalCount.set(jobs.length);
	} catch {
		pendingApprovalCount.set(0);
	}
}

export const sessions = writable<SessionSummary[]>([]);
export const activeSession = writable<SessionSummary | null>(null);
export const sidebarOpen = writable(false);

/** When true and the icon rail is expanded, department links are hidden behind one control (less vertical scroll). */
export const iconRailDeptsCollapsed = writable(false);
export const panelOpen = writable(true);
export const sidebarWidth = writable(256);
export const panelWidth = writable(288);
export const departments = writable<DepartmentDef[]>([]);
export const commandPaletteOpen = writable(false);
export const pendingCommand = writable<{ prompt: string } | null>(null);

/** Sprint S-020: right context panel on `/dept/*` (chat, properties, execution). */
export type ContextPanelMode = 'chat' | 'properties' | 'execution';

export const contextPanelOpen = writable(false);
export const contextPanelMode = writable<ContextPanelMode>('chat');
/** JSON payload for Properties mode; set from main content when an entity is selected. */
export const contextPanelProperties = writable<unknown>(null);

/** Sprint S-021: collapsible bottom panel on `/dept/*` (terminal, jobs, events). */
export type BottomPanelTab = 'terminal' | 'jobs' | 'events';

export const bottomPanelOpen = writable(false);
export const bottomPanelTab = writable<BottomPanelTab>('terminal');

// ── Onboarding state ─────────────────────────────────────────
export interface OnboardingState {
	sessionCreated: boolean;
	goalAdded: boolean;
	planGenerated: boolean;
	deptChatUsed: boolean;
	agentCreated: boolean;
	dismissed: boolean;
	tourCompleted: boolean;
}

const defaultOnboarding: OnboardingState = {
	sessionCreated: false,
	goalAdded: false,
	planGenerated: false,
	deptChatUsed: false,
	agentCreated: false,
	dismissed: false,
	tourCompleted: false
};

function loadOnboarding(): OnboardingState {
	if (typeof localStorage === 'undefined') return defaultOnboarding;
	try {
		const raw = localStorage.getItem('rusvel-onboarding');
		return raw ? { ...defaultOnboarding, ...JSON.parse(raw) } : defaultOnboarding;
	} catch {
		return defaultOnboarding;
	}
}

function createOnboardingStore() {
	const { subscribe, set, update } = writable<OnboardingState>(loadOnboarding());
	return {
		subscribe,
		set,
		update,
		complete(step: keyof OnboardingState) {
			update((s) => {
				const next = { ...s, [step]: true };
				if (typeof localStorage !== 'undefined') {
					localStorage.setItem('rusvel-onboarding', JSON.stringify(next));
				}
				return next;
			});
		},
		dismiss() {
			update((s) => {
				const next = { ...s, dismissed: true };
				if (typeof localStorage !== 'undefined') {
					localStorage.setItem('rusvel-onboarding', JSON.stringify(next));
				}
				return next;
			});
		}
	};
}

export const onboarding = createOnboardingStore();
