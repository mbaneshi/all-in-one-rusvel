<script lang="ts">
	import { untrack } from 'svelte';
	import DeptTerminal from '$lib/components/DeptTerminal.svelte';
	import { terminalDeptPaneUrl } from '$lib/clientTerminalApi';

	let {
		dept,
		sessionId
	}: {
		dept: string;
		sessionId: string | null;
	} = $props();

	let terminalPaneId = $state<string | null>(null);
	let terminalPaneForKey = $state<string | null>(null);
	let terminalLoading = $state(false);
	let terminalErr = $state('');

	const OPEN_TIMEOUT_MS = 25_000;

	$effect(() => {
		const sid = sessionId;
		const d = dept;
		if (!sid) {
			untrack(() => {
				terminalPaneId = null;
				terminalPaneForKey = null;
				terminalErr = '';
				terminalLoading = false;
			});
			return;
		}
		const key = `${sid}:${d}`;
		if (untrack(() => terminalPaneForKey === key && terminalPaneId !== null)) {
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
			.then((j: { pane_id?: string }) => {
				if (!cancelled && j.pane_id) {
					terminalPaneId = j.pane_id;
					terminalPaneForKey = key;
				} else if (!cancelled && !j.pane_id) {
					throw new Error('No pane_id in response');
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
</script>

<div class="flex h-full min-h-0 min-w-0 flex-1 flex-col p-2">
	{#if !sessionId}
		<p class="text-[11px] text-muted-foreground">Select a session to use the terminal.</p>
	{:else if terminalLoading}
		<p class="text-[11px] text-muted-foreground">Starting terminal…</p>
	{:else if terminalErr}
		<p class="text-[11px] text-red-500">{terminalErr}</p>
	{:else if terminalPaneId}
		<div class="min-h-0 min-w-0 flex-1">
			{#key terminalPaneId}
				<DeptTerminal paneId={terminalPaneId} />
			{/key}
		</div>
	{/if}
</div>
