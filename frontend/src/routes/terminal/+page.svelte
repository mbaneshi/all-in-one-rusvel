<script lang="ts">
	import { onMount } from 'svelte';
	import { get } from 'svelte/store';
	import { activeSession } from '$lib/stores';
	import { terminalDeptPaneUrl } from '$lib/clientTerminalApi';
	import DeptTerminal from '$lib/components/DeptTerminal.svelte';
	import { STANDALONE_TERMINAL_DEPT_ID } from '$lib/terminalConstants';

	let currentSession: import('$lib/api').SessionSummary | null = $state(get(activeSession));
	let terminalPaneId = $state<string | null>(null);
	let terminalLoading = $state(false);
	let terminalErr = $state('');

	async function openPane(sessionId: string): Promise<void> {
		terminalLoading = true;
		terminalErr = '';
		try {
			const url = terminalDeptPaneUrl(STANDALONE_TERMINAL_DEPT_ID, sessionId);
			const r = await fetch(url);
			if (!r.ok) {
				const t = await r.text();
				throw new Error(t || r.statusText);
			}
			const j = await r.json();
			if (j.pane_id) terminalPaneId = j.pane_id;
		} catch (e: unknown) {
			terminalErr = e instanceof Error ? e.message : 'Failed to open terminal';
		} finally {
			terminalLoading = false;
		}
	}

	onMount(() => {
		const unsub = activeSession.subscribe((v) => {
			currentSession = v;
			if (v?.id) {
				openPane(v.id);
			} else {
				terminalPaneId = null;
				terminalErr = '';
				terminalLoading = false;
			}
		});
		return unsub;
	});
</script>

<svelte:head>
	<title>Terminal | RUSVEL</title>
</svelte:head>

<div class="flex h-[calc(100vh-3.5rem)] flex-col">
	<div class="flex items-center justify-between border-b border-border px-4 py-2">
		<h1 class="text-sm font-semibold">Terminal</h1>
	</div>

	<div class="min-h-0 flex-1 p-2">
		{#if !currentSession?.id}
			<p class="text-xs text-muted-foreground">Select a session in the sidebar to use the terminal.</p>
		{:else if terminalLoading}
			<p class="text-xs text-muted-foreground">Starting terminal…</p>
		{:else if terminalErr}
			<p class="text-xs text-red-500">{terminalErr}</p>
		{:else if terminalPaneId}
			{#key terminalPaneId}
				<div class="h-full min-h-[320px]">
					<DeptTerminal paneId={terminalPaneId} />
				</div>
			{/key}
		{/if}
	</div>
</div>
