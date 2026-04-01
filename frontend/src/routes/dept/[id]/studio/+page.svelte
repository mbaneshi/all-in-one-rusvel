<script lang="ts">
	import { page } from '$app/state';
	import { activeSession } from '$lib/stores';
	import {
		postContentDraft,
		getContentList,
		ingestKnowledge,
		getKnowledge,
		type ContentItemRow,
		type KnowledgeEntry
	} from '$lib/api';
	import { toast } from 'svelte-sonner';

	let sessionId = $state<string | null>(null);
	activeSession.subscribe((s) => (sessionId = s?.id ?? null));

	let deptId = $derived(page.params.id);
	let isContent = $derived(deptId === 'content');

	// ── Capture state ──
	let captureText = $state('');
	let captureTags = $state('');
	let capturing = $state(false);

	// ── Draft state ──
	let anchorTopic = $state('');
	let selectedKinds: Record<string, boolean> = $state({
		LinkedInPost: true,
		Thread: true,
		Blog: true
	});
	let drafting = $state(false);
	let draftResults: ContentItemRow[] = $state([]);

	// ── Content list ──
	let contentItems: ContentItemRow[] = $state([]);
	let listLoading = $state(false);

	// ── Captures (from knowledge base) ──
	let captures: KnowledgeEntry[] = $state([]);
	let capturesLoading = $state(false);

	const PLATFORM_KINDS = [
		{ id: 'LinkedInPost', label: 'LinkedIn', color: 'border-sky-500/50 bg-sky-500/10' },
		{ id: 'Thread', label: 'Tweet Thread', color: 'border-cyan-500/50 bg-cyan-500/10' },
		{ id: 'Blog', label: 'DEV.to / Blog', color: 'border-emerald-500/50 bg-emerald-500/10' },
		{ id: 'Tweet', label: 'Single Tweet', color: 'border-cyan-500/30 bg-cyan-500/5' }
	];

	async function handleCapture() {
		if (!captureText.trim() || !sessionId) return;
		capturing = true;
		try {
			const source = captureTags.trim() || 'capture';
			await ingestKnowledge(captureText.trim(), source);
			toast.success('Captured to knowledge base');
			captureText = '';
			captureTags = '';
			await loadCaptures();
		} catch (e) {
			toast.error(e instanceof Error ? e.message : 'Capture failed');
		} finally {
			capturing = false;
		}
	}

	async function loadCaptures() {
		capturesLoading = true;
		try {
			const res = await getKnowledge();
			captures = Array.isArray(res) ? res : [];
		} catch {
			captures = [];
		} finally {
			capturesLoading = false;
		}
	}

	async function handleDraft() {
		if (!anchorTopic.trim() || !sessionId) return;
		const kinds = Object.entries(selectedKinds)
			.filter(([, v]) => v)
			.map(([k]) => k);
		if (kinds.length === 0) {
			toast.error('Select at least one platform');
			return;
		}
		drafting = true;
		draftResults = [];
		try {
			for (const kind of kinds) {
				const item = (await postContentDraft(sessionId, anchorTopic, kind)) as ContentItemRow;
				draftResults = [...draftResults, item];
			}
			toast.success(`${draftResults.length} drafts generated`);
			await loadContent();
		} catch (e) {
			toast.error(e instanceof Error ? e.message : 'Draft failed');
		} finally {
			drafting = false;
		}
	}

	function useCapture(text: string) {
		anchorTopic = text;
		toast.info('Loaded capture as topic');
	}

	async function loadContent() {
		if (!sessionId) return;
		listLoading = true;
		try {
			contentItems = await getContentList(sessionId);
		} catch {
			contentItems = [];
		} finally {
			listLoading = false;
		}
	}

	function statusBadge(status: string): string {
		switch (status) {
			case 'Draft':
				return 'border-yellow-500/50 bg-yellow-500/10 text-yellow-200';
			case 'Adapted':
				return 'border-blue-500/50 bg-blue-500/10 text-blue-200';
			case 'Published':
				return 'border-green-500/50 bg-green-500/10 text-green-200';
			default:
				return 'border-border bg-secondary/50 text-muted-foreground';
		}
	}

	function kindLabel(kind: string): string {
		return PLATFORM_KINDS.find((p) => p.id === kind)?.label ?? kind;
	}

	$effect(() => {
		if (sessionId && isContent) {
			void loadCaptures();
			void loadContent();
		}
	});
</script>

<div class="flex h-full min-h-0 flex-col overflow-auto">
	<!-- Header -->
	<div class="border-b border-border px-4 py-3">
		<h1 class="text-lg font-semibold">Content Studio</h1>
		<p class="text-xs text-muted-foreground">
			Capture ideas, generate platform variants, schedule and publish.
		</p>
	</div>

	{#if !isContent}
		<div class="flex flex-1 items-center justify-center p-6">
			<p class="text-sm text-muted-foreground">
				Studio is available at <span class="font-mono text-foreground">/dept/content/studio</span>.
			</p>
		</div>
	{:else if !sessionId}
		<div class="flex flex-1 items-center justify-center p-6">
			<p class="text-sm text-muted-foreground">Select a session in the top bar.</p>
		</div>
	{:else}
		<div class="grid flex-1 grid-cols-1 gap-4 overflow-auto p-4 lg:grid-cols-2">
			<!-- Left: Capture + Captures List -->
			<div class="flex flex-col gap-4">
				<!-- Quick Capture -->
				<div class="rounded-lg border border-border p-4">
					<h2 class="mb-2 text-sm font-semibold">Quick Capture</h2>
					<p class="mb-3 text-xs text-muted-foreground">
						Just finished something interesting? Dump your thoughts here. Raw notes, learnings,
						insights — no formatting needed.
					</p>
					<textarea
						class="w-full rounded-md border border-border bg-muted/30 p-3 text-sm leading-relaxed placeholder:text-muted-foreground/50 focus:border-primary/50 focus:outline-none"
						rows="5"
						placeholder="What did you learn today? What surprised you? What was the tradeoff?"
						bind:value={captureText}
					></textarea>
					<div class="mt-2 flex items-center gap-2">
						<input
							type="text"
							class="flex-1 rounded-md border border-border bg-muted/30 px-3 py-1.5 text-xs placeholder:text-muted-foreground/50 focus:border-primary/50 focus:outline-none"
							placeholder="Tags (e.g. rust, ai-agents, architecture)"
							bind:value={captureTags}
						/>
						<button
							type="button"
							class="rounded-md bg-primary px-4 py-1.5 text-xs font-medium text-primary-foreground hover:bg-primary/90 disabled:opacity-50"
							onclick={handleCapture}
							disabled={capturing || !captureText.trim()}
						>
							{capturing ? 'Saving...' : 'Capture'}
						</button>
					</div>
				</div>

				<!-- Recent Captures -->
				<div class="rounded-lg border border-border p-4">
					<h2 class="mb-2 text-sm font-semibold">Recent Captures</h2>
					{#if capturesLoading}
						<p class="text-xs text-muted-foreground">Loading...</p>
					{:else if captures.length === 0}
						<p class="text-xs text-muted-foreground">
							No captures yet. Use the form above to save your first insight.
						</p>
					{:else}
						<ul class="space-y-2">
							{#each captures.slice(0, 8) as cap}
								<li class="group rounded-md border border-border/50 p-2">
									<p class="line-clamp-3 text-xs leading-relaxed text-foreground/90">
										{cap.content}
									</p>
									<div class="mt-1 flex items-center justify-between">
										<span class="text-[10px] text-muted-foreground">{cap.source}</span>
										<button
											type="button"
											class="text-[10px] text-primary opacity-0 transition group-hover:opacity-100"
											onclick={() => useCapture(cap.content)}
										>
											Use as topic
										</button>
									</div>
								</li>
							{/each}
						</ul>
					{/if}
				</div>
			</div>

			<!-- Right: Generate + Results -->
			<div class="flex flex-col gap-4">
				<!-- Generate Variants -->
				<div class="rounded-lg border border-border p-4">
					<h2 class="mb-2 text-sm font-semibold">Generate Content</h2>
					<p class="mb-3 text-xs text-muted-foreground">
						Paste your anchor topic or click "Use as topic" on a capture. Select platforms and
						generate drafts.
					</p>
					<textarea
						class="w-full rounded-md border border-border bg-muted/30 p-3 text-sm leading-relaxed placeholder:text-muted-foreground/50 focus:border-primary/50 focus:outline-none"
						rows="4"
						placeholder="Topic: e.g. 'How I built a streaming agent runtime in Rust with deferred tool loading'"
						bind:value={anchorTopic}
					></textarea>
					<div class="mt-3 flex flex-wrap gap-2">
						{#each PLATFORM_KINDS as pk}
							<label
								class="flex cursor-pointer items-center gap-1.5 rounded-md border px-3 py-1.5 text-xs transition {selectedKinds[
									pk.id
								]
									? pk.color + ' font-medium'
									: 'border-border/50 text-muted-foreground opacity-60'}"
							>
								<input
									type="checkbox"
									class="hidden"
									bind:checked={selectedKinds[pk.id]}
								/>
								{pk.label}
							</label>
						{/each}
					</div>
					<div class="mt-3">
						<button
							type="button"
							class="rounded-md bg-primary px-4 py-2 text-sm font-medium text-primary-foreground hover:bg-primary/90 disabled:opacity-50"
							onclick={handleDraft}
							disabled={drafting || !anchorTopic.trim()}
						>
							{drafting ? 'Generating...' : 'Generate Drafts'}
						</button>
					</div>
				</div>

				<!-- Draft Results -->
				{#if draftResults.length > 0}
					<div class="rounded-lg border border-primary/30 bg-primary/5 p-4">
						<h2 class="mb-2 text-sm font-semibold">Generated Drafts</h2>
						<div class="space-y-3">
							{#each draftResults as item}
								<div class="rounded-md border border-border bg-background p-3">
									<div class="mb-1 flex items-center gap-2">
										<span class="text-xs font-medium">{kindLabel(item.kind)}</span>
										<span
											class="rounded border px-1.5 py-0.5 text-[10px] {statusBadge(
												item.status
											)}"
										>
											{item.status}
										</span>
									</div>
									<h3 class="mb-1 text-sm font-medium">{item.title || '(Untitled)'}</h3>
									<pre
										class="max-h-[200px] overflow-auto whitespace-pre-wrap rounded border border-border/50 bg-muted/20 p-2 text-xs leading-relaxed"
									>{item.body_markdown}</pre>
								</div>
							{/each}
						</div>
					</div>
				{/if}

				<!-- Existing Content -->
				<div class="rounded-lg border border-border p-4">
					<div class="mb-2 flex items-center justify-between">
						<h2 class="text-sm font-semibold">All Content</h2>
						<button
							type="button"
							class="text-[10px] text-primary hover:underline"
							onclick={() => void loadContent()}
						>
							Refresh
						</button>
					</div>
					{#if listLoading}
						<p class="text-xs text-muted-foreground">Loading...</p>
					{:else if contentItems.length === 0}
						<p class="text-xs text-muted-foreground">
							No content yet. Generate your first drafts above.
						</p>
					{:else}
						<ul class="space-y-1.5">
							{#each contentItems.slice(0, 15) as item}
								<li
									class="flex items-center justify-between rounded-md border border-border/50 px-2 py-1.5"
								>
									<div class="min-w-0 flex-1">
										<p class="truncate text-xs font-medium">
											{item.title || '(Untitled)'}
										</p>
										<p class="text-[10px] text-muted-foreground">
											{kindLabel(item.kind)}
											{#if item.published_at}
												· Published {new Date(item.published_at).toLocaleDateString()}
											{:else if item.scheduled_at}
												· Scheduled {new Date(item.scheduled_at).toLocaleDateString()}
											{/if}
										</p>
									</div>
									<span
										class="ml-2 shrink-0 rounded border px-1.5 py-0.5 text-[10px] {statusBadge(
											item.status
										)}"
									>
										{item.status}
									</span>
								</li>
							{/each}
						</ul>
					{/if}
				</div>
			</div>
		</div>
	{/if}
</div>
