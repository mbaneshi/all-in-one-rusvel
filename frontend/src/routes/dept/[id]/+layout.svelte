<script lang="ts">
	import type { Snippet } from 'svelte';
	import { onMount } from 'svelte';
	import { browser } from '$app/environment';
	import { page } from '$app/state';
	import { departments, contextPanelOpen, bottomPanelOpen } from '$lib/stores';
	import type { DepartmentDef } from '$lib/api';
	import {
		MessageSquare,
		Sliders,
		LayoutGrid,
		Calendar,
		Kanban,
		Users,
		Mail,
		Receipt,
		PanelRightOpen,
		PanelBottomOpen,
		Zap,
		Cpu,
		Terminal,
		Bot,
		GitBranch,
		BookOpen,
		Scale,
		Plug,
		Link2,
		FolderOpen,
		Activity
	} from 'lucide-svelte';
	import { deptExtraSections, deptShellNavItems, isDeptShellTabVisible } from '$lib/departmentManifest';
	import ContextPanel from '$lib/components/shell/ContextPanel.svelte';
	import BottomPanel from '$lib/components/shell/BottomPanel.svelte';

	let allDepts: DepartmentDef[] = $state([]);
	departments.subscribe((v) => (allDepts = v));

	let { children }: { children: Snippet } = $props();

	let dept = $derived(allDepts.find((d) => d.id === page.params.id));
	let base = $derived(`/dept/${page.params.id}`);

	let extras = $derived(deptExtraSections[page.params.id ?? ''] ?? []);

	function sectionActive(segment: string): boolean {
		const p = page.url.pathname;
		if (segment === 'chat') return p.endsWith('/chat');
		return p.endsWith(`/${segment}`);
	}

	onMount(() => {
		if (!browser) return;
		const onKey = (e: KeyboardEvent) => {
			const el = e.target as HTMLElement | null;
			if (el?.closest('input, textarea, [contenteditable="true"]')) return;
			if (!(e.metaKey || e.ctrlKey)) return;
			if (e.key.toLowerCase() === 'j') {
				e.preventDefault();
				contextPanelOpen.update((v) => !v);
				return;
			}
			if (e.code === 'Backquote') {
				e.preventDefault();
				bottomPanelOpen.update((v) => !v);
			}
		};
		window.addEventListener('keydown', onKey);
		return () => window.removeEventListener('keydown', onKey);
	});
</script>

{#if !dept}
	<div class="flex h-full items-center justify-center">
		<p class="text-sm text-[var(--muted-foreground)]">Department not found.</p>
	</div>
{:else}
	<div class="flex h-full min-h-0 flex-col">
		<div class="flex min-h-0 flex-1 overflow-hidden">
		<aside
			class="flex h-full min-h-0 w-48 shrink-0 flex-col gap-1 border-r border-border bg-sidebar/40 px-2 py-3"
			aria-label="Department sections"
		>
			<p class="mb-1 shrink-0 px-2 text-[10px] font-medium uppercase tracking-wide text-muted-foreground">
				{dept.name}
			</p>
			<div class="flex min-h-0 flex-1 flex-col gap-1 overflow-y-auto">
			<a
				href="{base}/chat"
				class="flex shrink-0 items-center gap-2 rounded-md px-2 py-2 text-sm transition-colors
					{sectionActive('chat')
					? 'bg-sidebar-primary/15 text-sidebar-primary font-medium'
					: 'text-muted-foreground hover:bg-sidebar-accent hover:text-sidebar-accent-foreground'}"
			>
				<MessageSquare size={16} strokeWidth={1.75} class="shrink-0" />
				Chat
			</a>
			{#each deptShellNavItems as item}
				{#if isDeptShellTabVisible(item.id, dept)}
					<a
						href="{base}/{item.pathSegment}"
						class="flex shrink-0 items-center gap-2 rounded-md px-2 py-2 text-sm transition-colors
							{sectionActive(item.pathSegment)
							? 'bg-sidebar-primary/15 text-sidebar-primary font-medium'
							: 'text-muted-foreground hover:bg-sidebar-accent hover:text-sidebar-accent-foreground'}"
					>
						{#if item.id === 'actions'}
							<Zap size={16} strokeWidth={1.75} class="shrink-0" />
						{:else if item.id === 'engine'}
							<Cpu size={16} strokeWidth={1.75} class="shrink-0" />
						{:else if item.id === 'terminal'}
							<Terminal size={16} strokeWidth={1.75} class="shrink-0" />
						{:else if item.id === 'agents'}
							<Bot size={16} strokeWidth={1.75} class="shrink-0" />
						{:else if item.id === 'workflows'}
							<GitBranch size={16} strokeWidth={1.75} class="shrink-0" />
						{:else if item.id === 'skills'}
							<BookOpen size={16} strokeWidth={1.75} class="shrink-0" />
						{:else if item.id === 'rules'}
							<Scale size={16} strokeWidth={1.75} class="shrink-0" />
						{:else if item.id === 'mcp'}
							<Plug size={16} strokeWidth={1.75} class="shrink-0" />
						{:else if item.id === 'hooks'}
							<Link2 size={16} strokeWidth={1.75} class="shrink-0" />
						{:else if item.id === 'projects'}
							<FolderOpen size={16} strokeWidth={1.75} class="shrink-0" />
						{:else if item.id === 'events'}
							<Activity size={16} strokeWidth={1.75} class="shrink-0" />
						{/if}
						{item.label}
					</a>
				{/if}
			{/each}
			<a
				href="{base}/config"
				class="flex shrink-0 items-center gap-2 rounded-md px-2 py-2 text-sm transition-colors
					{sectionActive('config')
					? 'bg-sidebar-primary/15 text-sidebar-primary font-medium'
					: 'text-muted-foreground hover:bg-sidebar-accent hover:text-sidebar-accent-foreground'}"
			>
				<Sliders size={16} strokeWidth={1.75} class="shrink-0" />
				Config
			</a>
			{#each extras as ex}
				<a
					href="{base}/{ex.segment}"
					class="flex shrink-0 items-center gap-2 rounded-md px-2 py-2 text-sm transition-colors
						{sectionActive(ex.segment)
						? 'bg-sidebar-primary/15 text-sidebar-primary font-medium'
						: 'text-muted-foreground hover:bg-sidebar-accent hover:text-sidebar-accent-foreground'}"
				>
					{#if ex.segment === 'studio'}
						<BookOpen size={16} strokeWidth={1.75} class="shrink-0" />
					{:else if ex.segment === 'pipeline'}
						<LayoutGrid size={16} strokeWidth={1.75} class="shrink-0" />
					{:else if ex.segment === 'calendar'}
						<Calendar size={16} strokeWidth={1.75} class="shrink-0" />
					{:else if ex.segment === 'deals'}
						<Kanban size={16} strokeWidth={1.75} class="shrink-0" />
					{:else if ex.segment === 'contacts'}
						<Users size={16} strokeWidth={1.75} class="shrink-0" />
					{:else if ex.segment === 'outreach'}
						<Mail size={16} strokeWidth={1.75} class="shrink-0" />
					{:else if ex.segment === 'invoices'}
						<Receipt size={16} strokeWidth={1.75} class="shrink-0" />
					{/if}
					{ex.label}
				</a>
			{/each}
			</div>

			<button
				type="button"
				class="flex items-center gap-2 rounded-md px-2 py-2 text-left text-xs text-muted-foreground transition-colors hover:bg-sidebar-accent hover:text-foreground"
				onclick={() => contextPanelOpen.update((v) => !v)}
				title="Toggle context panel (⌘J / Ctrl+J)"
			>
				<PanelRightOpen size={16} strokeWidth={1.75} class="shrink-0" />
				<span>Context</span>
				<kbd class="ml-auto hidden rounded border border-border bg-secondary/80 px-1 font-mono text-[9px] lg:inline"
					>⌘J</kbd
				>
			</button>
			<button
				type="button"
				class="flex items-center gap-2 rounded-md px-2 py-2 text-left text-xs text-muted-foreground transition-colors hover:bg-sidebar-accent hover:text-foreground"
				onclick={() => bottomPanelOpen.update((v) => !v)}
				title="Toggle bottom panel (⌘` / Ctrl+`)"
			>
				<PanelBottomOpen size={16} strokeWidth={1.75} class="shrink-0" />
				<span>Bottom</span>
				<kbd class="ml-auto hidden rounded border border-border bg-secondary/80 px-1 font-mono text-[9px] lg:inline"
					>⌘`</kbd
				>
			</button>
		</aside>

		<div class="flex min-h-0 min-w-0 flex-1">
			<div class="min-h-0 min-w-0 flex-1 overflow-hidden">
				{@render children()}
			</div>
			{#if $contextPanelOpen}
				<ContextPanel deptId={dept.id} deptTitle={dept.title} />
			{:else}
				<button
					type="button"
					class="flex w-9 shrink-0 flex-col items-center justify-center gap-1 border-l border-border bg-muted/20 py-2 text-muted-foreground hover:bg-muted/40"
					onclick={() => contextPanelOpen.set(true)}
					title="Open context panel (⌘J)"
				>
					<PanelRightOpen size={18} strokeWidth={1.75} />
				</button>
			{/if}
		</div>
		</div>

		{#if $bottomPanelOpen}
			<BottomPanel deptId={dept.id} />
		{:else}
			<button
				type="button"
				class="flex h-7 w-full shrink-0 items-center justify-center gap-2 border-t border-border bg-muted/20 text-[10px] text-muted-foreground hover:bg-muted/35"
				onclick={() => bottomPanelOpen.set(true)}
				title="Open bottom panel: terminal, jobs, events (⌘`)"
			>
				<PanelBottomOpen size={14} strokeWidth={1.75} />
				<span>Terminal · Jobs · Events</span>
				<kbd class="rounded border border-border bg-secondary/80 px-1 font-mono text-[9px]">⌘`</kbd>
			</button>
		{/if}
	</div>
{/if}
