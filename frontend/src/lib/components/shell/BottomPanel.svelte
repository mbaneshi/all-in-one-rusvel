<script lang="ts">
	import { onMount } from 'svelte';
	import { get } from 'svelte/store';
	import { TerminalSquare, ListTodo, Activity, X } from 'lucide-svelte';
	import DeptTerminal from '$lib/components/DeptTerminal.svelte';
	import { activeSession, bottomPanelOpen, bottomPanelTab } from '$lib/stores';
	import type { BottomPanelTab } from '$lib/stores';
	import { terminalDeptPaneUrl } from '$lib/clientTerminalApi';
	import { getDeptEvents, getJobs, type Event, type JobListItem } from '$lib/api';
	import Button from '$lib/components/ui/Button.svelte';

	let { deptId }: { deptId: string } = $props();

	let sessionId = $state<string | null>(null);

	let terminalPaneId = $state<string | null>(null);
	let terminalKey = $state<string | null>(null);
	let terminalLoading = $state(false);
	let terminalErr = $state('');

	let jobs = $state<JobListItem[]>([]);
	let jobsLoading = $state(false);
	let jobsErr = $state('');

	let events = $state<Event[]>([]);
	let eventsLoading = $state(false);
	let eventsErr = $state('');

	const tabs: { id: BottomPanelTab; label: string; icon: typeof TerminalSquare }[] = [
		{ id: 'terminal', label: 'Terminal', icon: TerminalSquare },
		{ id: 'jobs', label: 'Jobs', icon: ListTodo },
		{ id: 'events', label: 'Events', icon: Activity }
	];

	function close() {
		bottomPanelOpen.set(false);
	}

	function setTab(t: BottomPanelTab) {
		bottomPanelTab.set(t);
		if (t === 'jobs') void loadJobs();
		if (t === 'events') void loadEvents();
	}

	async function loadJobs() {
		if (!sessionId) {
			jobs = [];
			return;
		}
		jobsLoading = true;
		jobsErr = '';
		try {
			jobs = await getJobs(sessionId, { limit: 40 });
		} catch (e) {
			jobsErr = e instanceof Error ? e.message : 'Failed to load jobs';
			jobs = [];
		} finally {
			jobsLoading = false;
		}
	}

	async function loadEvents() {
		eventsLoading = true;
		eventsErr = '';
		try {
			events = await getDeptEvents(deptId);
		} catch (e) {
			eventsErr = e instanceof Error ? e.message : 'Failed to load events';
			events = [];
		} finally {
			eventsLoading = false;
		}
	}

	$effect(() => {
		if (!sessionId || !deptId) {
			terminalPaneId = null;
			terminalKey = null;
			terminalErr = '';
			terminalLoading = false;
			return;
		}
		const key = `${sessionId}:${deptId}`;
		if (terminalKey === key && terminalPaneId) return;

		let cancelled = false;
		terminalLoading = true;
		terminalErr = '';
		const url = terminalDeptPaneUrl(deptId, sessionId);
		fetch(url)
			.then((r) => {
				if (!r.ok) return r.text().then((t) => Promise.reject(new Error(t || r.statusText)));
				return r.json();
			})
			.then((j: { pane_id?: string }) => {
				if (!cancelled && j.pane_id) {
					terminalPaneId = j.pane_id;
					terminalKey = key;
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

	onMount(() => {
		const unsubSession = activeSession.subscribe((s) => {
			sessionId = s?.id ?? null;
			if (s?.id && get(bottomPanelTab) === 'jobs') void loadJobs();
		});

		const poll = setInterval(() => {
			if (get(bottomPanelTab) === 'events') void loadEvents();
		}, 15000);

		return () => {
			unsubSession();
			clearInterval(poll);
		};
	});
</script>

<div
	class="flex h-[200px] shrink-0 flex-col border-t border-border bg-card"
	role="region"
	aria-label="Bottom panel"
>
	<div class="flex items-center justify-between gap-2 border-b border-border px-2 py-1">
		<div class="flex gap-0.5">
			{#each tabs as tab}
				{@const Icon = tab.icon}
				<button
					type="button"
					onclick={() => setTab(tab.id)}
					class="flex items-center gap-1 rounded-md px-2 py-1 text-[10px] font-medium transition-colors
						{$bottomPanelTab === tab.id
						? 'bg-sidebar-primary/20 text-sidebar-primary'
						: 'text-muted-foreground hover:bg-accent hover:text-foreground'}"
				>
					<Icon size={12} strokeWidth={2} class="shrink-0" />
					{tab.label}
				</button>
			{/each}
		</div>
		<button
			type="button"
			onclick={close}
			class="rounded-md p-1 text-muted-foreground hover:bg-accent hover:text-foreground"
			title="Close panel (⌘`)"
		>
			<X size={16} strokeWidth={1.75} />
		</button>
	</div>

	<div class="min-h-0 flex-1 overflow-hidden p-1">
		{#if $bottomPanelTab === 'terminal'}
			{#if !sessionId}
				<p class="p-2 text-[10px] text-muted-foreground">Select a session for the department terminal.</p>
			{:else if terminalLoading}
				<p class="p-2 text-[10px] text-muted-foreground">Starting terminal…</p>
			{:else if terminalErr}
				<p class="p-2 text-[10px] text-red-500">{terminalErr}</p>
			{:else if terminalPaneId}
				{#key terminalPaneId}
					<div class="h-[calc(200px-2.5rem)] min-h-[120px]">
						<DeptTerminal paneId={terminalPaneId} />
					</div>
				{/key}
			{/if}
		{:else if $bottomPanelTab === 'jobs'}
			<div class="flex h-full flex-col gap-1 overflow-hidden">
				<div class="flex items-center justify-between px-1">
					<span class="text-[10px] text-muted-foreground">Session job queue</span>
					<Button variant="ghost" size="sm" class="!h-6 !px-2 !text-[10px]" onclick={() => loadJobs()}>
						Refresh
					</Button>
				</div>
				{#if jobsLoading}
					<p class="text-[10px] text-muted-foreground">Loading…</p>
				{:else if jobsErr}
					<p class="text-[10px] text-red-500">{jobsErr}</p>
				{:else if jobs.length === 0}
					<p class="text-[10px] text-muted-foreground">No jobs for this session.</p>
				{:else}
					<div class="min-h-0 flex-1 overflow-auto font-mono text-[9px]">
						<table class="w-full border-collapse text-left">
							<thead class="sticky top-0 bg-card text-muted-foreground">
								<tr>
									<th class="border-b border-border py-0.5 pr-2">id</th>
									<th class="border-b border-border py-0.5 pr-2">kind</th>
									<th class="border-b border-border py-0.5">status</th>
								</tr>
							</thead>
							<tbody>
								{#each jobs as j}
									<tr class="border-b border-border/60">
										<td class="max-w-[7rem] truncate py-0.5 pr-2">{j.id}</td>
										<td class="py-0.5 pr-2">{j.kind}</td>
										<td class="py-0.5">{j.status}</td>
									</tr>
								{/each}
							</tbody>
						</table>
					</div>
				{/if}
			</div>
		{:else}
			<div class="flex h-full flex-col gap-1 overflow-hidden">
				<div class="flex items-center justify-between px-1">
					<span class="text-[10px] text-muted-foreground">Department events</span>
					<Button variant="ghost" size="sm" class="!h-6 !px-2 !text-[10px]" onclick={() => loadEvents()}>
						Refresh
					</Button>
				</div>
				{#if eventsLoading}
					<p class="text-[10px] text-muted-foreground">Loading…</p>
				{:else if eventsErr}
					<p class="text-[10px] text-red-500">{eventsErr}</p>
				{:else if events.length === 0}
					<p class="text-[10px] text-muted-foreground">No events yet.</p>
				{:else}
					<div class="min-h-0 flex-1 space-y-1 overflow-auto text-[9px]">
						{#each events.slice(0, 50) as ev}
							<div class="rounded border border-border/60 bg-muted/20 px-1.5 py-1">
								<div class="flex justify-between gap-1 text-muted-foreground">
									<span class="truncate font-medium text-foreground">{ev.kind}</span>
									<span class="shrink-0">{ev.created_at?.slice(11, 19) ?? ''}</span>
								</div>
								<div class="truncate text-muted-foreground">{ev.source}</div>
							</div>
						{/each}
					</div>
				{/if}
			</div>
		{/if}
	</div>
</div>
