/** Mirrors `rusvel_core::registry::QuickAction`. */
export interface QuickAction {
	label: string;
	prompt: string;
}

/** Mirrors `rusvel_core::department::UiContribution` (subset used by the shell). */
export interface UiContribution {
	tabs: string[];
	dashboard_cards?: DashboardCard[];
	has_settings?: boolean;
	custom_components?: string[];
}

export interface DashboardCard {
	title: string;
	description: string;
	size?: string;
}

/**
 * Department record from `GET /api/departments` — aligned with registry `DepartmentDef`
 * and forward-compatible with manifest-shaped payloads (`ui.tabs`).
 */
export interface DepartmentDef {
	id: string;
	name: string;
	title: string;
	icon: string;
	color: string;
	system_prompt: string;
	capabilities: string[];
	tabs: string[];
	quick_actions: QuickAction[];
	default_config: Record<string, unknown>;
	ui?: UiContribution;
}

/** Normalize API JSON whether the backend sends flat `DepartmentDef` or manifest-style `ui.tabs`. */
export function normalizeDepartment(raw: Record<string, unknown>): DepartmentDef {
	const ui = raw.ui as UiContribution | undefined;
	const tabsFromUi = ui?.tabs?.length ? ui.tabs : undefined;
	const flat = raw.tabs as string[] | undefined;
	const tabs = tabsFromUi ?? flat ?? [];

	const base = {
		...raw,
		tabs
	} as DepartmentDef;

	if (ui) {
		base.ui = { ...ui, tabs };
	}
	return base;
}

export function normalizeDepartmentList(raw: unknown): DepartmentDef[] {
	if (!Array.isArray(raw)) return [];
	return raw.map((x) => normalizeDepartment(x as Record<string, unknown>));
}

/** Effective tab IDs for the department panel (manifest-first). */
export function tabsFromDepartment(d: DepartmentDef): string[] {
	if (d.ui?.tabs?.length) return d.ui.tabs;
	return d.tabs ?? [];
}

export function deptHref(deptId: string): string {
	return `/dept/${encodeURIComponent(deptId)}/actions`;
}

/** Ordered shell nav for department section routes (matches former DepartmentPanel tabs). */
export const deptShellNavItems: { id: string; label: string; pathSegment: string }[] = [
	{ id: 'actions', label: 'Actions', pathSegment: 'actions' },
	{ id: 'engine', label: 'Engine', pathSegment: 'engine' },
	{ id: 'terminal', label: 'Terminal', pathSegment: 'terminal' },
	{ id: 'agents', label: 'Agents', pathSegment: 'agents' },
	{ id: 'workflows', label: 'Agent chains', pathSegment: 'workflows' },
	{ id: 'skills', label: 'Skills', pathSegment: 'skills' },
	{ id: 'rules', label: 'Rules', pathSegment: 'rules' },
	{ id: 'mcp', label: 'MCP', pathSegment: 'mcp' },
	{ id: 'hooks', label: 'Hooks', pathSegment: 'hooks' },
	{ id: 'projects', label: 'Dirs', pathSegment: 'dirs' },
	{ id: 'events', label: 'Events', pathSegment: 'events' }
];

/** Whether a panel tab should appear in the dept shell for this department. */
export function isDeptShellTabVisible(tabId: string, d: DepartmentDef): boolean {
	const t = tabsFromDepartment(d);
	if (tabId === 'projects') return t.includes('projects') || t.includes('dirs');
	if (tabId === 'terminal') return true;
	return t.includes(tabId);
}

/** Optional dept sub-routes (pipeline, calendar) — single registry for the shell. */
export const deptExtraSections: Record<string, { segment: string; label: string }[]> = {
	harvest: [{ segment: 'pipeline', label: 'Pipeline' }],
	content: [{ segment: 'calendar', label: 'Calendar' }],
	gtm: [
		{ segment: 'contacts', label: 'Contacts' },
		{ segment: 'outreach', label: 'Outreach' },
		{ segment: 'deals', label: 'Deals' },
		{ segment: 'invoices', label: 'Invoices' }
	]
};

/**
 * Pick a department id for deep links when the registry order or IDs may differ.
 * Prefers `preferred` if present; otherwise first department; otherwise `fallback`.
 */
export function resolveDeptId(
	depts: DepartmentDef[],
	preferred: string,
	fallback: string
): string {
	if (depts.some((d) => d.id === preferred)) return preferred;
	return depts[0]?.id ?? fallback;
}
