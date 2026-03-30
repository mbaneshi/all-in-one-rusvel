<script lang="ts">
	import { page } from '$app/state';
	import { getActiveDashboard, type ActiveDashboardResponse } from '$lib/api';
	import { activeSession } from '$lib/stores';
	import { ClipboardList } from 'lucide-svelte';

	let data = $state<ActiveDashboardResponse | null>(null);
	let err = $state('');
	let loading = $state(true);

	let sessionId = $state<string | null>(null);
	activeSession.subscribe((s) => (sessionId = s?.id ?? null));

	const highlightJobId = $derived(page.url.searchParams.get('job')?.trim() ?? '');

	async function load() {
		loading = true;
		err = '';
		try {
			data = await getActiveDashboard(sessionId);
		} catch (e) {
			err = e instanceof Error ? e.message : String(e);
			data = null;
		} finally {
			loading = false;
		}
	}

	$effect(() => {
		void sessionId;
		void load();
	});

	$effect(() => {
		if (!highlightJobId || loading || !data?.jobs.length) return;
		queueMicrotask(() => {
			const el = document.getElementById(`job-row-${highlightJobId}`);
			el?.scrollIntoView({ block: 'center', behavior: 'smooth' });
		});
	});
</script>

<div class="h-full overflow-auto p-6">
	<div class="mb-6 flex items-center gap-3">
		<ClipboardList class="h-7 w-7 text-muted-foreground" strokeWidth={1.75} />
		<div>
			<h1 class="text-xl font-semibold text-foreground">Active tasks</h1>
			<p class="text-sm text-muted-foreground">
				In-flight jobs, approval queue, and cron schedules (Cowork-style overview).
			</p>
		</div>
		<button
			type="button"
			class="ml-auto rounded-md border border-border px-3 py-1.5 text-sm hover:bg-secondary"
			onclick={() => load()}
		>
			Refresh
		</button>
	</div>

	{#if loading}
		<p class="text-sm text-muted-foreground">Loading…</p>
	{:else if err}
		<p class="text-sm text-destructive">{err}</p>
	{:else if data}
		<section class="mb-8">
			<h2 class="mb-2 text-sm font-semibold uppercase tracking-wide text-muted-foreground">
				Jobs (queued / running / awaiting approval)
			</h2>
			{#if data.jobs.length === 0}
				<p class="text-sm text-muted-foreground">No active jobs.</p>
			{:else}
				<ul class="space-y-2 text-sm">
					{#each data.jobs as j}
						<li
							id="job-row-{j.id}"
							class="rounded-lg border bg-card px-3 py-2 font-mono text-xs {highlightJobId === j.id
								? 'border-primary ring-2 ring-primary/40'
								: 'border-border'}"
						>
							<span class="text-foreground">{j.kind}</span>
							<span class="mx-2 text-muted-foreground">·</span>
							<span class="text-muted-foreground">{j.status}</span>
							<span class="mx-2 text-muted-foreground">·</span>
							<span class="text-muted-foreground">{j.id}</span>
						</li>
					{/each}
				</ul>
			{/if}
		</section>

		<section class="mb-8">
			<h2 class="mb-2 text-sm font-semibold uppercase tracking-wide text-muted-foreground">
				Pending approvals
			</h2>
			{#if !Array.isArray(data.pending_approvals) || data.pending_approvals.length === 0}
				<p class="text-sm text-muted-foreground">None — or use <a href="/approvals" class="underline">Approvals</a>.</p>
			{:else}
				<p class="text-sm text-muted-foreground">
					{data.pending_approvals.length} job(s) — open
					<a href="/approvals" class="underline">Approvals</a>.
				</p>
			{/if}
		</section>

		<section>
			<h2 class="mb-2 text-sm font-semibold uppercase tracking-wide text-muted-foreground">
				Cron schedules
			</h2>
			{#if !Array.isArray(data.cron_schedules) || data.cron_schedules.length === 0}
				<p class="text-sm text-muted-foreground">No schedules.</p>
			{:else}
				<pre class="max-h-64 overflow-auto rounded-lg border border-border bg-secondary p-3 text-xs">{JSON.stringify(
						data.cron_schedules,
						null,
						2
					)}</pre>
			{/if}
		</section>
	{/if}
</div>
