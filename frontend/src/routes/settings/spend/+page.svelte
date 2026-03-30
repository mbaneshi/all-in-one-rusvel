<script lang="ts">
	import { onMount } from 'svelte';
	import { Chart, Svg, Axis, Bars } from 'layerchart';
	import { activeSession } from '$lib/stores';
	import { getAnalyticsSpend, type AnalyticsSpendResponse } from '$lib/api';
	import { toast } from 'svelte-sonner';

	let currentSession: import('$lib/api').SessionSummary | null = $state(null);
	activeSession.subscribe((v) => (currentSession = v));

	let spend: AnalyticsSpendResponse | null = $state(null);
	let loading = $state(true);

	let costByDeptRows = $derived.by(() => {
		if (!spend?.department_tokens || Object.keys(spend.department_tokens).length === 0) {
			return [] as { key: string; total_usd: number; total_tokens: number }[];
		}
		const tok = spend.department_tokens;
		return Object.entries(spend.by_department)
			.map(([key, total_usd]) => ({
				key,
				total_usd,
				total_tokens: tok[key] ?? 0
			}))
			.sort((a, b) => b.total_usd - a.total_usd);
	});

	let costByModelRows = $derived.by(() => {
		if (!spend?.by_model || Object.keys(spend.by_model).length === 0) {
			return [] as { key: string; total_usd: number; total_tokens: number }[];
		}
		return Object.entries(spend.by_model)
			.map(([key, m]) => ({
				key,
				total_usd: m.usd,
				total_tokens: m.tokens
			}))
			.sort((a, b) => b.total_usd - a.total_usd);
	});

	let chartRows = $derived.by(() => {
		if (!spend) return [] as { dept: string; usd: number }[];
		return Object.entries(spend.by_department)
			.sort((a, b) => b[1] - a[1])
			.map(([dept, usd]) => ({ dept, usd }));
	});

	let maxY = $derived.by(() => {
		const rows = chartRows;
		if (rows.length === 0) return 1;
		const m = Math.max(...rows.map((d: { dept: string; usd: number }) => d.usd));
		return m > 0 ? m * 1.08 : 1;
	});

	async function load() {
		loading = true;
		try {
			spend = await getAnalyticsSpend(undefined, currentSession?.id ?? null);
		} catch (e) {
			toast.error(e instanceof Error ? e.message : 'Failed to load spend');
			spend = null;
		} finally {
			loading = false;
		}
	}

	onMount(() => {
		load();
		return activeSession.subscribe(() => load());
	});
</script>

<div class="space-y-6">
	<div>
		<h2 class="text-lg font-semibold text-foreground">Spend</h2>
		<p class="mt-1 text-sm text-muted-foreground leading-relaxed">
			Aggregated from SQLite <code class="rounded bg-muted px-1">cost_events</code> when present; otherwise
			from legacy metrics (<code class="rounded bg-muted px-1">llm.cost_usd</code>). Optional session scope
			includes budget hints from session config.
		</p>
	</div>

	{#if loading}
		<p class="text-muted-foreground">Loading…</p>
	{:else if spend}
		<div class="max-w-2xl space-y-6">
			<div class="rounded-xl border border-border bg-card p-5">
				<h2 class="mb-3 text-sm font-semibold uppercase tracking-wider text-muted-foreground">
					Totals
				</h2>
				<p class="text-3xl font-bold text-foreground">${spend.total_usd.toFixed(4)}</p>
				<p class="mt-1 text-xs text-muted-foreground">All departments (or filtered by query)</p>
			</div>

			{#if spend.session_id != null && spend.session_total_usd != null}
				<div
					class="rounded-xl border p-5 {spend.budget_warning
						? 'border-amber-500/50 bg-amber-500/10'
						: 'border-border bg-card'}"
				>
					<h2 class="mb-2 text-sm font-semibold uppercase tracking-wider text-muted-foreground">
						Active session
					</h2>
					<p class="text-lg font-medium text-foreground">
						${spend.session_total_usd.toFixed(4)}
						{#if spend.session_budget_limit_usd != null}
							<span class="text-muted-foreground">
								/ ${spend.session_budget_limit_usd.toFixed(2)} budget</span
							>
						{/if}
					</p>
					{#if spend.budget_warning}
						<p class="mt-2 text-sm text-amber-600 dark:text-amber-400">
							At or above 80% of configured session budget.
						</p>
					{/if}
				</div>
			{/if}

			<div class="rounded-xl border border-border bg-card p-5">
				<h2 class="mb-3 text-sm font-semibold uppercase tracking-wider text-muted-foreground">
					By department
				</h2>
				{#if chartRows.length > 0}
					<div class="mb-6 h-64 w-full min-w-0">
						<Chart
							data={chartRows}
							x="dept"
							y="usd"
							yDomain={[0, maxY]}
							padding={{ left: 56, bottom: 52, top: 8, right: 12 }}
						>
							<Svg>
								<Axis placement="left" grid rule />
								<Axis placement="bottom" rule />
								<Bars radius={4} class="fill-primary/85" />
							</Svg>
						</Chart>
					</div>
				{/if}
				<p class="sr-only">Department spend table (same data as chart)</p>
				<ul class="space-y-2">
					{#each Object.entries(spend.by_department).sort((a, b) => b[1] - a[1]) as [dept, usd]}
						<li class="flex justify-between rounded-lg bg-secondary/40 px-3 py-2 text-sm">
							<span class="font-mono text-foreground">{dept}</span>
							<span class="text-muted-foreground">${usd.toFixed(4)}</span>
						</li>
					{/each}
				</ul>
			</div>

			<div class="rounded-xl border border-border bg-card p-5">
				<h2 class="mb-3 text-sm font-semibold uppercase tracking-wider text-muted-foreground">
					Cost breakdown
				</h2>
				<p class="mb-4 text-xs text-muted-foreground">
					Token-aware breakdown is included in <code class="rounded bg-muted px-1">GET /api/analytics/spend</code>
					when <code class="rounded bg-muted px-1">cost_events</code> has data.
				</p>
				<h3 class="mb-2 text-xs font-medium text-foreground">By department</h3>
				<div class="mb-6 overflow-x-auto">
					<table class="w-full text-left text-sm">
						<thead>
							<tr class="border-b border-border text-muted-foreground">
								<th class="py-2 pr-4 font-medium">Department</th>
								<th class="py-2 pr-4 font-medium">Total spend</th>
								<th class="py-2 font-medium">Tokens</th>
							</tr>
						</thead>
						<tbody>
							{#each costByDeptRows as row}
								<tr class="border-b border-border/60">
									<td class="py-2 pr-4 font-mono text-foreground">{row.key}</td>
									<td class="py-2 pr-4 text-muted-foreground">${row.total_usd.toFixed(4)}</td>
									<td class="py-2 text-muted-foreground">{row.total_tokens.toLocaleString()}</td>
								</tr>
							{/each}
						</tbody>
					</table>
					{#if costByDeptRows.length === 0}
						<p class="mt-2 text-xs text-muted-foreground">No cost events yet.</p>
					{/if}
				</div>
				<h3 class="mb-2 text-xs font-medium text-foreground">By model</h3>
				<div class="overflow-x-auto">
					<table class="w-full text-left text-sm">
						<thead>
							<tr class="border-b border-border text-muted-foreground">
								<th class="py-2 pr-4 font-medium">Model</th>
								<th class="py-2 pr-4 font-medium">Total spend</th>
								<th class="py-2 font-medium">Tokens</th>
							</tr>
						</thead>
						<tbody>
							{#each costByModelRows as row}
								<tr class="border-b border-border/60">
									<td class="py-2 pr-4 font-mono text-foreground">{row.key}</td>
									<td class="py-2 pr-4 text-muted-foreground">${row.total_usd.toFixed(4)}</td>
									<td class="py-2 text-muted-foreground">{row.total_tokens.toLocaleString()}</td>
								</tr>
							{/each}
						</tbody>
					</table>
					{#if costByModelRows.length === 0}
						<p class="mt-2 text-xs text-muted-foreground">No cost events yet.</p>
					{/if}
				</div>
			</div>
		</div>
	{:else}
		<p class="text-muted-foreground">No data.</p>
	{/if}
</div>
