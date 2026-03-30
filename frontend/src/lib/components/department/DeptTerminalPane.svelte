<script lang="ts">
	import { beforeNavigate } from '$app/navigation';
	import { untrack } from 'svelte';
	import { postTerminalDeptTrace } from '$lib/api';
	import DeptTerminal from '$lib/components/DeptTerminal.svelte';
	import {
		terminalDeptPaneUrl,
		terminalWindowAddPaneUrl,
		terminalWindowLayoutUrl
	} from '$lib/clientTerminalApi';

	let {
		dept,
		sessionId
	}: {
		dept: string;
		sessionId: string | null;
	} = $props();

	let paneIds = $state<string[]>([]);
	let windowId = $state<string | null>(null);
	let terminalPaneForKey = $state<string | null>(null);
	let terminalLoading = $state(false);
	let terminalErr = $state('');
	let addingPane = $state(false);
	let traceUiToTerminal = $state(false);

	const OPEN_TIMEOUT_MS = 25_000;

	beforeNavigate(({ to }) => {
		if (!traceUiToTerminal || !sessionId) return;
		const path = to?.url?.pathname;
		if (path) {
			void postTerminalDeptTrace(dept, sessionId, `nav ${path}`);
		}
	});

	$effect(() => {
		const sid = sessionId;
		const d = dept;
		if (!sid) {
			untrack(() => {
				paneIds = [];
				windowId = null;
				terminalPaneForKey = null;
				terminalErr = '';
				terminalLoading = false;
			});
			return;
		}
		const key = `${sid}:${d}`;
		if (untrack(() => terminalPaneForKey === key && paneIds.length > 0 && windowId !== null)) {
			return;
		}

		let cancelled = false;
		untrack(() => {
			terminalLoading = true;
			terminalErr = '';
		});
		const url = terminalDeptPaneUrl(d, sid);
		const ac = new AbortController();
		const timer = setTimeout(() => ac.abort(), OPEN_TIMEOUT_MS);
		fetch(url, { signal: ac.signal })
			.then((r) => {
				if (!r.ok) return r.text().then((t) => Promise.reject(new Error(t || r.statusText)));
				return r.json();
			})
			.then((j: { pane_id?: string; window_id?: string }) => {
				if (!cancelled && j.pane_id && j.window_id) {
					paneIds = [j.pane_id];
					windowId = j.window_id;
					terminalPaneForKey = key;
				} else if (!cancelled && (!j.pane_id || !j.window_id)) {
					throw new Error('Invalid terminal response (need pane_id and window_id)');
				}
			})
			.catch((e: unknown) => {
				if (!cancelled) {
					if (e instanceof DOMException && e.name === 'AbortError') {
						terminalErr = `Opening terminal timed out after ${OPEN_TIMEOUT_MS / 1000}s (check API is reachable via this origin)`;
					} else {
						terminalErr = e instanceof Error ? e.message : 'Failed to open terminal';
					}
				}
			})
			.finally(() => {
				clearTimeout(timer);
				if (!cancelled) terminalLoading = false;
			});
		return () => {
			cancelled = true;
			clearTimeout(timer);
			ac.abort();
		};
	});

	async function addShell(): Promise<void> {
		const sid = sessionId;
		const wid = windowId;
		if (!sid || !wid) return;
		addingPane = true;
		terminalErr = '';
		try {
			const r = await fetch(terminalWindowAddPaneUrl(wid, sid), {
				method: 'POST',
				headers: { 'Content-Type': 'application/json' },
				body: JSON.stringify({})
			});
			if (!r.ok) {
				const t = await r.text();
				throw new Error(t || r.statusText);
			}
			const j = (await r.json()) as { pane_id?: string };
			if (j.pane_id) paneIds = [...paneIds, j.pane_id];
		} catch (e: unknown) {
			terminalErr = e instanceof Error ? e.message : 'Failed to add pane';
		} finally {
			addingPane = false;
		}
	}

	async function tileVertical(): Promise<void> {
		const sid = sessionId;
		const wid = windowId;
		if (!sid || !wid || paneIds.length < 2) return;
		const n = paneIds.length;
		const ratio = 1 / n;
		const body = { type: 'VSplit', value: Array.from({ length: n }, () => ratio) };
		try {
			const r = await fetch(terminalWindowLayoutUrl(wid, sid), {
				method: 'POST',
				headers: { 'Content-Type': 'application/json' },
				body: JSON.stringify(body)
			});
			if (!r.ok && r.status !== 204) {
				const t = await r.text();
				throw new Error(t || r.statusText);
			}
		} catch (e: unknown) {
			terminalErr = e instanceof Error ? e.message : 'Failed to set layout';
		}
	}
</script>

<div class="flex h-full min-h-0 min-w-0 flex-1 flex-col p-2">
	{#if !sessionId}
		<p class="text-[11px] text-muted-foreground">Select a session to use the terminal.</p>
	{:else if terminalLoading}
		<p class="text-[11px] text-muted-foreground">Starting terminal…</p>
	{:else if terminalErr}
		<p class="text-[11px] text-red-500">{terminalErr}</p>
	{:else if paneIds.length > 0 && windowId}
		<div class="flex min-h-0 min-w-0 flex-1 flex-col gap-1">
			<div class="flex shrink-0 flex-wrap items-center gap-2">
				<button
					type="button"
					disabled={addingPane}
					class="rounded border border-border bg-secondary/80 px-2 py-1 text-[10px] font-medium text-foreground hover:bg-secondary disabled:opacity-50"
					onclick={() => addShell()}
				>
					{addingPane ? 'Adding…' : '+ Shell'}
				</button>
				{#if paneIds.length > 1}
					<button
						type="button"
						class="rounded border border-border bg-secondary/80 px-2 py-1 text-[10px] font-medium text-foreground hover:bg-secondary"
						onclick={() => tileVertical()}
					>
						Tile vertical
					</button>
				{/if}
				<label
					class="flex cursor-pointer items-center gap-1 text-[10px] text-muted-foreground"
				>
					<input
						type="checkbox"
						bind:checked={traceUiToTerminal}
						class="rounded border-border"
					/>
					Trace UI navigation
				</label>
			</div>
			<div
				class="grid min-h-0 min-w-0 flex-1 gap-1 {paneIds.length > 1
					? 'grid-cols-1 sm:grid-cols-2'
					: ''}"
			>
				{#each paneIds as pid (pid)}
					<div class="min-h-[200px] min-w-0">
						{#key pid}
							<DeptTerminal paneId={pid} />
						{/key}
					</div>
				{/each}
			</div>
		</div>
	{/if}
</div>
