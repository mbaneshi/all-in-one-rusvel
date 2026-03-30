/**
 * Restore department terminal windows from `GET /api/terminal/session/snapshot`
 * so reload keeps splits instead of always creating a fresh pane via `/terminal/dept/...`.
 */
import { terminalSessionSnapshotUrl, terminalWindowLayoutUrl } from '$lib/clientTerminalApi';

export type TerminalSnapshotWindow = {
	id: string;
	dept_id?: string | null;
	name?: string;
	panes?: string[];
	layout?: unknown;
};

export type TerminalSnapshotPane = {
	id: string;
	window_id: string;
	source?: { type: string; value?: unknown };
};

export type DeptTerminalRestore = {
	windowId: string;
	paneIds: string[];
	layout: unknown;
};

export async function fetchDeptTerminalFromSnapshot(
	sessionId: string,
	deptId: string
): Promise<DeptTerminalRestore | null> {
	const r = await fetch(terminalSessionSnapshotUrl(sessionId));
	if (!r.ok) return null;

	const j = (await r.json()) as {
		windows?: TerminalSnapshotWindow[];
		panes?: TerminalSnapshotPane[];
	};
	const windows = j.windows ?? [];
	const panes = j.panes ?? [];

	const win =
		windows.find((w) => w.dept_id === deptId) ??
		windows.find((w) => w.name === `dept-${deptId}`);

	if (!win?.id) return null;

	let paneIds = (win.panes ?? []).filter((id): id is string => typeof id === 'string' && id.length > 0);

	if (paneIds.length === 0) {
		paneIds = panes
			.filter((p) => p.window_id === win.id)
			.filter((p) => {
				if (p.source?.type === 'Department') {
					return p.source.value === deptId;
				}
				return true;
			})
			.map((p) => p.id)
			.filter((id) => typeof id === 'string' && id.length > 0);
	}

	if (paneIds.length === 0) return null;

	return {
		windowId: win.id,
		paneIds,
		layout: win.layout ?? null
	};
}

/** Re-apply server layout if it matches pane count (e.g. after page reload). */
export async function syncTerminalLayoutFromSnapshot(
	windowId: string,
	sessionId: string,
	layout: unknown,
	paneCount: number
): Promise<void> {
	if (!layout || typeof layout !== 'object' || paneCount < 2) return;
	const o = layout as { type?: string; value?: unknown };
	const t = o.type;
	if (t !== 'VSplit' && t !== 'HSplit') return;
	const value = o.value;
	if (!Array.isArray(value) || value.length !== paneCount) return;

	const r = await fetch(terminalWindowLayoutUrl(windowId, sessionId), {
		method: 'POST',
		headers: { 'Content-Type': 'application/json' },
		body: JSON.stringify(layout)
	});
	if (!r.ok && r.status !== 204) {
		console.warn('terminal layout restore failed', r.status);
	}
}
