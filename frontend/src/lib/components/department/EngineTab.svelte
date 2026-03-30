<script lang="ts">
	import { activeSession } from '$lib/stores';
	import {
		getBrowserStatus,
		getCodeSearch,
		getContentList,
		getHarvestOpportunities,
		getHarvestPipeline,
		postBrowserConnect,
		postCodeAnalyze,
		postContentDraft,
		postForgePipeline,
		postHarvestScan
	} from '$lib/api';
	import { toast } from 'svelte-sonner';

	let { dept, deptHsl }: { dept: string; deptHsl: string } = $props();

	let currentSession: import('$lib/api').SessionSummary | null = $state(null);
	activeSession.subscribe((v) => (currentSession = v));

	let busy = $state(false);
	let output = $state('');
	let codePath = $state('.');
	let codeQuery = $state('');
	let codeLimit = $state(20);
	let contentTopic = $state('');
	let contentKind = $state('Blog');
	let forgeDraftTopic = $state('');
	let harvestSources = $state<Set<string>>(new Set(['freelancer']));
	let harvestQuery = $state('');
	let cdpEndpoint = $state('http://127.0.0.1:9223');
	let browserConnected = $state<boolean | null>(null);

	function showJson(data: unknown) {
		output = JSON.stringify(data, null, 2);
	}

	async function runCodeAnalyze() {
		busy = true;
		output = '';
		try {
			const r = await postCodeAnalyze(codePath.trim() || '.');
			showJson(r);
		} catch (e) {
			output = e instanceof Error ? e.message : String(e);
			toast.error(output);
		} finally {
			busy = false;
		}
	}

	async function runCodeSearch() {
		if (!codeQuery.trim()) return;
		busy = true;
		output = '';
		try {
			const r = await getCodeSearch(codeQuery.trim(), codeLimit);
			showJson(r);
		} catch (e) {
			output = e instanceof Error ? e.message : String(e);
			toast.error(output);
		} finally {
			busy = false;
		}
	}

	async function runContentDraft() {
		if (!currentSession) {
			toast.error('Select a session first');
			return;
		}
		if (!contentTopic.trim()) return;
		busy = true;
		output = '';
		try {
			const r = await postContentDraft(
				currentSession.id,
				contentTopic.trim(),
				contentKind || undefined
			);
			showJson(r);
		} catch (e) {
			output = e instanceof Error ? e.message : String(e);
			toast.error(output);
		} finally {
			busy = false;
		}
	}

	async function runContentList() {
		if (!currentSession) {
			toast.error('Select a session first');
			return;
		}
		busy = true;
		output = '';
		try {
			const r = await getContentList(currentSession.id);
			showJson(r);
		} catch (e) {
			output = e instanceof Error ? e.message : String(e);
			toast.error(output);
		} finally {
			busy = false;
		}
	}

	async function runHarvestPipeline() {
		if (!currentSession) {
			toast.error('Select a session first');
			return;
		}
		busy = true;
		output = '';
		try {
			const r = await getHarvestPipeline(currentSession.id);
			showJson(r);
		} catch (e) {
			output = e instanceof Error ? e.message : String(e);
			toast.error(output);
		} finally {
			busy = false;
		}
	}

	function toggleHarvestSource(id: string) {
		const next = new Set(harvestSources);
		if (next.has(id)) next.delete(id);
		else next.add(id);
		harvestSources = next;
	}

	async function refreshBrowserStatus() {
		try {
			const s = await getBrowserStatus();
			browserConnected = s.connected;
		} catch {
			browserConnected = null;
		}
	}

	async function connectBrowser() {
		busy = true;
		output = '';
		try {
			const r = await postBrowserConnect(cdpEndpoint.trim() || 'http://127.0.0.1:9223');
			showJson(r);
			await refreshBrowserStatus();
			toast.success('CDP connected');
		} catch (e) {
			output = e instanceof Error ? e.message : String(e);
			toast.error(output);
		} finally {
			busy = false;
		}
	}

	async function runHarvestScan() {
		if (!currentSession) {
			toast.error('Select a session first');
			return;
		}
		const sources = [...harvestSources];
		if (sources.length === 0) {
			toast.error('Pick at least one source');
			return;
		}
		if (sources.includes('cdp') && !harvestQuery.trim()) {
			toast.error('CDP source needs a listing page URL in Query / URL');
			return;
		}
		busy = true;
		output = '';
		try {
			const rows = await postHarvestScan({
				session_id: currentSession.id,
				sources,
				query: harvestQuery.trim(),
				cdp_endpoint: cdpEndpoint.trim() || undefined
			});
			showJson(rows);
			toast.success(`Scan done — ${rows.length} opportunity row(s) returned`);
		} catch (e) {
			output = e instanceof Error ? e.message : String(e);
			toast.error(output);
		} finally {
			busy = false;
		}
	}

	async function listHarvestOpportunities() {
		if (!currentSession) {
			toast.error('Select a session first');
			return;
		}
		busy = true;
		output = '';
		try {
			const rows = await getHarvestOpportunities(currentSession.id);
			showJson(rows);
		} catch (e) {
			output = e instanceof Error ? e.message : String(e);
			toast.error(output);
		} finally {
			busy = false;
		}
	}

	async function runForgePipeline() {
		if (!currentSession) {
			toast.error('Select a session first');
			return;
		}
		busy = true;
		output = '';
		try {
			const topic = forgeDraftTopic.trim();
			const r = await postForgePipeline(
				currentSession.id,
				topic ? { draft_topic: topic } : undefined
			);
			showJson(r);
		} catch (e) {
			output = e instanceof Error ? e.message : String(e);
			toast.error(output);
		} finally {
			busy = false;
		}
	}
</script>

<div class="p-3 space-y-3 text-[10px]">
	{#if dept === 'code'}
		<div class="space-y-2">
			<p class="font-medium text-foreground" style="color: hsl({deptHsl})">Code engine</p>
			<label class="block space-y-0.5">
				<span class="text-muted-foreground">Path</span>
				<input
					bind:value={codePath}
					class="w-full rounded border border-border bg-background px-2 py-1 font-mono text-[10px]"
				/>
			</label>
			<button
				type="button"
				disabled={busy}
				onclick={runCodeAnalyze}
				class="w-full rounded-md py-1.5 text-xs font-medium text-white disabled:opacity-50"
				style="background: hsl({deptHsl})"
			>
				Analyze
			</button>
		</div>
		<div class="space-y-2 border-t border-border pt-2">
			<label class="block space-y-0.5">
				<span class="text-muted-foreground">Search query</span>
				<input
					bind:value={codeQuery}
					class="w-full rounded border border-border bg-background px-2 py-1 font-mono text-[10px]"
				/>
			</label>
			<label class="block space-y-0.5">
				<span class="text-muted-foreground">Limit</span>
				<input
					type="number"
					bind:value={codeLimit}
					min="1"
					max="100"
					class="w-full rounded border border-border bg-background px-2 py-1"
				/>
			</label>
			<button
				type="button"
				disabled={busy}
				onclick={runCodeSearch}
				class="w-full rounded-md bg-secondary py-1.5 text-xs font-medium disabled:opacity-50"
			>
				Search
			</button>
		</div>
	{:else if dept === 'content'}
		<div class="space-y-2">
			<p class="font-medium text-foreground" style="color: hsl({deptHsl})">Content engine</p>
			{#if !currentSession}
				<p class="text-muted-foreground">Select a session in the header to draft or list.</p>
			{:else}
				<p class="text-muted-foreground">Session: {currentSession.name}</p>
			{/if}
			<label class="block space-y-0.5">
				<span class="text-muted-foreground">Topic</span>
				<input
					bind:value={contentTopic}
					class="w-full rounded border border-border bg-background px-2 py-1"
				/>
			</label>
			<label class="block space-y-0.5">
				<span class="text-muted-foreground">Kind</span>
				<select
					bind:value={contentKind}
					class="w-full rounded border border-border bg-background px-2 py-1"
				>
					<option value="Blog">Blog</option>
					<option value="LinkedInPost">LinkedInPost</option>
					<option value="Thread">Thread</option>
					<option value="Tweet">Tweet</option>
					<option value="LongForm">LongForm</option>
				</select>
			</label>
			<button
				type="button"
				disabled={busy || !currentSession}
				onclick={runContentDraft}
				class="w-full rounded-md py-1.5 text-xs font-medium text-white disabled:opacity-50"
				style="background: hsl({deptHsl})"
			>
				Draft
			</button>
			<button
				type="button"
				disabled={busy || !currentSession}
				onclick={runContentList}
				class="w-full rounded-md bg-secondary py-1.5 text-xs font-medium disabled:opacity-50"
			>
				List content
			</button>
		</div>
	{:else if dept === 'forge'}
		<div class="space-y-2">
			<p class="font-medium text-foreground" style="color: hsl({deptHsl})">Forge — opportunity pipeline</p>
			{#if !currentSession}
				<p class="text-muted-foreground">Select a session in the header.</p>
			{:else}
				<p class="text-muted-foreground">Session: {currentSession.name}</p>
			{/if}
			<label class="block space-y-0.5">
				<span class="text-muted-foreground">Optional draft topic (overrides harvest title)</span>
				<input
					bind:value={forgeDraftTopic}
					placeholder="Leave empty to use first scanned opportunity title"
					class="w-full rounded border border-border bg-background px-2 py-1"
				/>
			</label>
			<button
				type="button"
				disabled={busy || !currentSession}
				onclick={runForgePipeline}
				class="w-full rounded-md py-1.5 text-xs font-medium text-white disabled:opacity-50"
				style="background: hsl({deptHsl})"
			>
				Run opportunity pipeline
			</button>
			<p class="text-muted-foreground leading-relaxed">
				<code class="rounded bg-muted px-0.5">POST /api/forge/pipeline</code> — Harvest scan → score → propose →
				content draft.
			</p>
		</div>
	{:else if dept === 'harvest'}
		<div class="space-y-2">
			<p class="font-medium text-foreground" style="color: hsl({deptHsl})">Harvest engine</p>
			{#if !currentSession}
				<p class="text-muted-foreground">Select a session in the header.</p>
			{:else}
				<p class="text-muted-foreground">Session: {currentSession.name}</p>
			{/if}

			<div class="rounded-md border border-border bg-muted/20 p-2 space-y-1.5">
				<div class="flex flex-wrap items-center gap-2">
					<span class="text-muted-foreground">CDP</span>
					{#if browserConnected === true}
						<span class="text-emerald-500">connected</span>
					{:else if browserConnected === false}
						<span class="text-amber-500">not connected</span>
					{:else}
						<span class="text-muted-foreground">unknown</span>
					{/if}
					<button
						type="button"
						disabled={busy}
						onclick={() => void refreshBrowserStatus()}
						class="rounded border border-border bg-background px-1.5 py-0.5 text-[9px] hover:bg-accent"
					>
						Refresh status
					</button>
				</div>
				<label class="block space-y-0.5">
					<span class="text-muted-foreground">Chrome DevTools HTTP base</span>
					<input
						bind:value={cdpEndpoint}
						placeholder="http://127.0.0.1:9223"
						class="w-full rounded border border-border bg-background px-2 py-1 font-mono text-[10px]"
					/>
				</label>
				<button
					type="button"
					disabled={busy}
					onclick={() => void connectBrowser()}
					class="w-full rounded-md bg-secondary py-1.5 text-xs font-medium disabled:opacity-50"
				>
					Connect Chrome (CDP)
				</button>
			</div>

			<p class="text-muted-foreground leading-relaxed">
				Sources: <code class="rounded bg-muted px-0.5">mock</code> (demo),
				<code class="rounded bg-muted px-0.5">freelancer</code> /
				<code class="rounded bg-muted px-0.5">upwork</code> (RSS),
				<code class="rounded bg-muted px-0.5">cdp</code> (needs URL below + Chrome above).
			</p>
			<div class="flex flex-wrap gap-2">
				{#each ['mock', 'freelancer', 'upwork', 'cdp'] as src (src)}
					<label class="flex cursor-pointer items-center gap-1 text-[10px]">
						<input
							type="checkbox"
							checked={harvestSources.has(src)}
							onchange={() => toggleHarvestSource(src)}
							class="rounded border-border"
						/>
						<span>{src}</span>
					</label>
				{/each}
			</div>
			<label class="block space-y-0.5">
				<span class="text-muted-foreground">Query (RSS) or listing URL (CDP)</span>
				<input
					bind:value={harvestQuery}
					placeholder="e.g. rust — or https://www.freelancer.com/jobs/..."
					class="w-full rounded border border-border bg-background px-2 py-1 text-[10px]"
				/>
			</label>
			<button
				type="button"
				disabled={busy || !currentSession}
				onclick={() => void runHarvestScan()}
				class="w-full rounded-md py-1.5 text-xs font-medium text-white disabled:opacity-50"
				style="background: hsl({deptHsl})"
			>
				Run harvest scan
			</button>
			<button
				type="button"
				disabled={busy || !currentSession}
				onclick={() => void listHarvestOpportunities()}
				class="w-full rounded-md bg-secondary py-1.5 text-xs font-medium disabled:opacity-50"
			>
				List opportunities (session)
			</button>
			<button
				type="button"
				disabled={busy || !currentSession}
				onclick={runHarvestPipeline}
				class="w-full rounded-md border border-border py-1.5 text-xs font-medium disabled:opacity-50"
			>
				Refresh pipeline stats
			</button>
		</div>
	{:else}
		<p class="text-muted-foreground">No engine tools for this department.</p>
	{/if}

	{#if output}
		<pre
			class="max-h-64 overflow-auto rounded-md border border-border bg-muted/30 p-2 font-mono text-[9px] leading-relaxed whitespace-pre-wrap break-all"
		>{output}</pre>
	{/if}
</div>
