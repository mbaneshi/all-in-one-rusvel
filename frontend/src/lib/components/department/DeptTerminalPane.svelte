<script lang="ts">
	import DeptTerminal from '$lib/components/DeptTerminal.svelte';

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

	function apiBase(): string {
		if (typeof window === 'undefined') return '';
		const { protocol, hostname, port } = window.location;
		const apiPort = port === '5173' ? '3000' : port;
		return `${protocol}//${hostname}${apiPort ? `:${apiPort}` : ''}`;
	}

	$effect(() => {
		if (!sessionId) return;
		const key = `${sessionId}:${dept}`;
		if (terminalPaneForKey === key && terminalPaneId) return;

		let cancelled = false;
		terminalLoading = true;
		terminalErr = '';
		const url = `${apiBase()}/api/terminal/dept/${encodeURIComponent(dept)}?session_id=${encodeURIComponent(sessionId)}`;
		fetch(url)
			.then((r) => {
				if (!r.ok) return r.text().then((t) => Promise.reject(new Error(t || r.statusText)));
				return r.json();
			})
			.then((j: { pane_id?: string }) => {
				if (!cancelled && j.pane_id) {
					terminalPaneId = j.pane_id;
					terminalPaneForKey = key;
				}
			})
			.catch((e: unknown) => {
				if (!cancelled) terminalErr = e instanceof Error ? e.message : 'Failed to open terminal';
			})
			.finally(() => {
				if (!cancelled) terminalLoading = false;
			});
		return () => {
			cancelled = true;
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
