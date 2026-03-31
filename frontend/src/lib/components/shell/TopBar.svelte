<script lang="ts">
	import { onMount } from 'svelte';
	import { getSessions, createSession, getDepartments } from '$lib/api';
	import {
		sessions,
		activeSession,
		departments,
		refreshPendingApprovalCount,
		commandPaletteOpen
	} from '$lib/stores';
	import {
		themeState,
		setThemePreference,
		type ThemePreference
	} from '$lib/design/theme.svelte';
	let loading = $state(true);
	let error = $state('');
	let showNewSession = $state(false);
	let newName = $state('');
	let newKind = $state('Project');

	let currentSessions: import('$lib/api').SessionSummary[] = $state([]);
	let currentActive: import('$lib/api').SessionSummary | null = $state(null);
	sessions.subscribe((v) => (currentSessions = v));
	activeSession.subscribe((v) => (currentActive = v));

	onMount(async () => {
		try {
			const [list, depts] = await Promise.all([getSessions(), getDepartments()]);
			sessions.set(list);
			departments.set(depts);
			if (list.length > 0) {
				activeSession.set(list[0]);
			}
		} catch (e) {
			error = e instanceof Error ? e.message : 'Failed to load';
		} finally {
			loading = false;
		}
	});

	onMount(() => {
		void refreshPendingApprovalCount();
		const interval = setInterval(() => refreshPendingApprovalCount(), 45_000);
		const onFocus = () => refreshPendingApprovalCount();
		const onVisibility = () => {
			if (typeof document !== 'undefined' && document.visibilityState === 'visible') {
				void refreshPendingApprovalCount();
			}
		};
		if (typeof window !== 'undefined') {
			window.addEventListener('focus', onFocus);
			document.addEventListener('visibilitychange', onVisibility);
		}
		return () => {
			clearInterval(interval);
			if (typeof window !== 'undefined') {
				window.removeEventListener('focus', onFocus);
				document.removeEventListener('visibilitychange', onVisibility);
			}
		};
	});

	async function handleCreateSession() {
		if (!newName.trim()) return;
		try {
			await createSession(newName.trim(), newKind);
			const list = await getSessions();
			sessions.set(list);
			if (!currentActive && list.length > 0) {
				activeSession.set(list[0]);
			}
			newName = '';
			showNewSession = false;
		} catch (e) {
			error = e instanceof Error ? e.message : 'Failed to create session';
		}
	}

	function selectSession(event: globalThis.Event) {
		const target = event.target as HTMLSelectElement;
		const id = target.value;
		const found = currentSessions.find((s) => s.id === id);
		if (found) activeSession.set(found);
	}

	const themeChoices: ThemePreference[] = ['system', 'light', 'dark'];

	function themeSegClass(p: ThemePreference) {
		return themeState.preference === p
			? 'border-primary/50 bg-primary/15 text-foreground'
			: 'border-transparent text-muted-foreground hover:bg-muted/50 hover:text-foreground';
	}
</script>

<header
	class="flex h-12 shrink-0 items-center gap-3 border-b border-border bg-sidebar px-3 text-sidebar-foreground"
>
	<div class="flex min-w-0 flex-1 items-center gap-2 overflow-x-auto" data-tour="session-switcher">
		{#if loading}
			<span class="text-xs text-muted-foreground">Sessions…</span>
		{:else if currentSessions.length === 0}
			<span class="text-xs text-muted-foreground">No sessions</span>
		{:else}
			<label for="topbar-session-select" class="sr-only">Session</label>
			<select
				id="topbar-session-select"
				onchange={selectSession}
				value={currentActive?.id ?? ''}
				class="max-w-[12rem] rounded-md border border-border bg-secondary px-2 py-1.5 text-xs text-foreground focus:border-primary focus:outline-none sm:max-w-xs"
			>
				{#each currentSessions as session}
					<option value={session.id}>{session.name}</option>
				{/each}
			</select>
		{/if}
		<button
			onclick={() => (showNewSession = !showNewSession)}
			class="shrink-0 rounded-md bg-secondary px-2 py-1 text-xs text-muted-foreground hover:bg-accent hover:text-accent-foreground"
		>
			+ New
		</button>
		{#if showNewSession}
			<div class="flex min-w-0 flex-wrap items-center gap-1.5">
				<input
					bind:value={newName}
					placeholder="Name"
					class="w-28 rounded-md border border-border bg-secondary px-2 py-1 text-xs text-foreground focus:border-primary focus:outline-none sm:w-36"
				/>
				<select
					bind:value={newKind}
					class="rounded-md border border-border bg-secondary px-1 py-1 text-xs text-foreground"
				>
					<option>Project</option>
					<option>Lead</option>
					<option>ContentCampaign</option>
					<option>General</option>
				</select>
				<button
					onclick={handleCreateSession}
					class="rounded-md bg-primary px-2 py-1 text-xs font-medium text-primary-foreground hover:bg-primary/90"
				>
					Create
				</button>
			</div>
		{/if}
		{#if error}
			<span class="text-xs text-destructive">{error}</span>
		{/if}
	</div>

	<div
		class="flex shrink-0 items-center rounded-md border border-border bg-secondary p-0.5"
		role="group"
		aria-label="Theme"
	>
		{#each themeChoices as p}
			<button
				type="button"
				onclick={() => setThemePreference(p)}
				class="rounded px-1.5 py-1 text-[10px] font-medium capitalize transition-colors {themeSegClass(p)}"
				aria-pressed={themeState.preference === p}
				title={p === 'system' ? 'Use system appearance' : `${p} mode`}
			>
				{p === 'system' ? 'Auto' : p}
			</button>
		{/each}
	</div>

	<button
		type="button"
		onclick={() => commandPaletteOpen.set(true)}
		class="hidden shrink-0 items-center gap-1 rounded-md border border-border bg-secondary px-2 py-1 text-[10px] text-muted-foreground hover:text-foreground sm:flex"
		aria-label="Open command palette"
	>
		<kbd class="rounded border border-border bg-background px-1 py-0.5 font-mono">⌘K</kbd>
	</button>
</header>
