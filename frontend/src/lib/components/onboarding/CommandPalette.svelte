<script lang="ts">
	import { onMount } from 'svelte';
	import { goto } from '$app/navigation';
	import { page } from '$app/state';
	import {
		commandPaletteOpen,
		departments,
		contextPanelOpen,
		bottomPanelOpen
	} from '$lib/stores';
	import { createSession, deptHref, resolveDeptId } from '$lib/api';
	import type { DepartmentDef } from '$lib/api';
	import { deptExtraSections, deptShellNavItems, isDeptShellTabVisible } from '$lib/departmentManifest';

	let isOpen = $state(false);
	let query = $state('');
	let selectedIndex = $state(0);
	let inputEl: HTMLInputElement | undefined = $state(undefined);

	commandPaletteOpen.subscribe((v) => {
		isOpen = v;
		if (v) {
			query = '';
			selectedIndex = 0;
			setTimeout(() => inputEl?.focus(), 50);
		}
	});

	let deptList: DepartmentDef[] = $state([]);
	departments.subscribe((v) => (deptList = v));

	let currentDeptId = $derived.by(() => {
		const m = page.url.pathname.match(/^\/dept\/([^/]+)/);
		return m ? decodeURIComponent(m[1]) : null;
	});

	let currentDept = $derived(
		currentDeptId ? (deptList.find((d) => d.id === currentDeptId) ?? null) : null
	);

	interface Command {
		id: string;
		label: string;
		group: string;
		icon: string;
		action: () => void;
	}

	function matchQuery(q: string, c: Command): boolean {
		if (q === '') return true;
		const needle = q.toLowerCase();
		return (
			c.label.toLowerCase().includes(needle) ||
			c.group.toLowerCase().includes(needle) ||
			c.id.toLowerCase().includes(needle)
		);
	}

	let commands = $derived.by((): Command[] => {
		const layout: Command[] = [
			{
				id: 'layout-context-panel',
				label: 'Toggle context panel',
				group: 'Layout',
				icon: 'J',
				action: () => {
					contextPanelOpen.update((v) => !v);
					close();
				}
			},
			{
				id: 'layout-bottom-panel',
				label: 'Toggle bottom panel',
				group: 'Layout',
				icon: '`',
				action: () => {
					bottomPanelOpen.update((v) => !v);
					close();
				}
			}
		];

		const thisDept: Command[] = [];
		if (currentDept) {
			const base = `/dept/${encodeURIComponent(currentDept.id)}`;
			thisDept.push({
				id: `here-${currentDept.id}-chat`,
				label: `${currentDept.title} — Chat`,
				group: 'This department',
				icon: '◆',
				action: () => navigate(`${base}/chat`)
			});
			for (const item of deptShellNavItems) {
				if (!isDeptShellTabVisible(item.id, currentDept)) continue;
				thisDept.push({
					id: `here-${currentDept.id}-${item.pathSegment}`,
					label: `${currentDept.title} — ${item.label}`,
					group: 'This department',
					icon: '·',
					action: () => navigate(`${base}/${item.pathSegment}`)
				});
			}
			thisDept.push({
				id: `here-${currentDept.id}-config`,
				label: `${currentDept.title} — Config`,
				group: 'This department',
				icon: '⚙',
				action: () => navigate(`${base}/config`)
			});
			for (const ex of deptExtraSections[currentDept.id] ?? []) {
				thisDept.push({
					id: `here-${currentDept.id}-${ex.segment}`,
					label: `${currentDept.title} — ${ex.label}`,
					group: 'This department',
					icon: '◇',
					action: () => navigate(`${base}/${ex.segment}`)
				});
			}
		}

		const deptNav: Command[] = deptList.map((d, i) => ({
			id: `nav-${d.id}`,
			label: i < 9 ? `${d.title} (Alt+${i + 1})` : d.title,
			group: 'Navigation',
			icon: d.icon,
			action: () => navigate(deptHref(d.id))
		}));
		const forgeId = resolveDeptId(deptList, 'forge', 'forge');
		return [
			...layout,
			...thisDept,
			{
				id: 'nav-dashboard',
				label: 'Dashboard',
				group: 'Navigation',
				icon: '~',
				action: () => navigate('/')
			},
			{
				id: 'nav-chat',
				label: 'Chat (God Agent)',
				group: 'Navigation',
				icon: '>',
				action: () => navigate('/chat')
			},
			...deptNav,
			{
				id: 'nav-settings',
				label: 'Settings',
				group: 'Navigation',
				icon: '⚙',
				action: () => navigate('/settings')
			},
			{
				id: 'nav-settings-llm',
				label: 'Settings — LLM & models',
				group: 'Navigation',
				icon: '⚙',
				action: () => navigate('/settings/llm')
			},
			{
				id: 'nav-settings-spend',
				label: 'Settings — Spend',
				group: 'Navigation',
				icon: '⚙',
				action: () => navigate('/settings/spend')
			},
			{
				id: 'act-new-session',
				label: 'Create New Session',
				group: 'Actions',
				icon: '+',
				action: () => handleCreateSession()
			},
			{
				id: 'act-plan',
				label: 'Generate Daily Plan',
				group: 'Actions',
				icon: '=',
				action: () => navigate(deptHref(forgeId))
			},
			{
				id: 'act-new-chat',
				label: 'New Chat',
				group: 'Actions',
				icon: '»',
				action: () => navigate('/chat')
			}
		];
	});

	let filtered = $derived(
		query.trim() === '' ? commands : commands.filter((c) => matchQuery(query.trim(), c))
	);

	let groups = $derived(
		[...new Set(filtered.map((c) => c.group))].map((g) => ({
			name: g,
			items: filtered.filter((c) => c.group === g)
		}))
	);

	function navigate(path: string) {
		close();
		goto(path);
	}

	function close() {
		commandPaletteOpen.set(false);
	}

	async function handleCreateSession() {
		close();
		const name = `session-${Date.now()}`;
		await createSession(name, 'General');
		goto('/');
	}

	function runSelected() {
		if (filtered[selectedIndex]) {
			filtered[selectedIndex].action();
		}
	}

	function handleKeydown(e: KeyboardEvent) {
		if (e.key === 'ArrowDown') {
			e.preventDefault();
			selectedIndex = Math.min(selectedIndex + 1, filtered.length - 1);
		} else if (e.key === 'ArrowUp') {
			e.preventDefault();
			selectedIndex = Math.max(selectedIndex - 1, 0);
		} else if (e.key === 'Enter') {
			e.preventDefault();
			runSelected();
		} else if (e.key === 'Escape') {
			close();
		}
	}

	onMount(() => {
		function globalKeydown(e: KeyboardEvent) {
			if ((e.metaKey || e.ctrlKey) && e.key === 'k') {
				e.preventDefault();
				commandPaletteOpen.update((v) => !v);
			}
		}
		window.addEventListener('keydown', globalKeydown);
		return () => window.removeEventListener('keydown', globalKeydown);
	});
</script>

{#if isOpen}
	<!-- Backdrop -->
	<div
		class="fixed inset-0 z-50 flex items-start justify-center bg-black/60 pt-[20vh]"
		onclick={close}
		onkeydown={(e) => e.key === 'Escape' && close()}
		role="dialog"
		tabindex="-1"
		aria-modal="true"
		aria-label="Command palette"
	>
		<!-- Panel -->
		<!-- svelte-ignore a11y_click_events_have_key_events -->
		<!-- svelte-ignore a11y_no_static_element_interactions -->
		<div
			class="w-full max-w-lg rounded-xl border border-[var(--border)] bg-[var(--card)] shadow-2xl"
			onclick={(e) => e.stopPropagation()}
		>
			<!-- Search input -->
			<div class="flex items-center gap-3 border-b border-[var(--border)] px-4 py-3">
				<svg
					class="h-4 w-4 text-[var(--muted-foreground)]"
					viewBox="0 0 16 16"
					fill="none"
					stroke="currentColor"
					stroke-width="1.5"><circle cx="6.5" cy="6.5" r="4" /><path d="M10 10l3.5 3.5" /></svg
				>
				<input
					bind:this={inputEl}
					bind:value={query}
					onkeydown={handleKeydown}
					placeholder="Search commands..."
					class="flex-1 bg-transparent text-sm text-[var(--foreground)] placeholder-[var(--muted-foreground)] focus:outline-none"
				/>
				<kbd
					class="rounded border border-[var(--border)] bg-[var(--secondary)] px-1.5 py-0.5 text-[10px] text-[var(--muted-foreground)]"
					>esc</kbd
				>
			</div>

			<!-- Results -->
			<div class="max-h-72 overflow-y-auto py-2">
				{#if filtered.length === 0}
					<p class="px-4 py-6 text-center text-sm text-[var(--muted-foreground)]">
						No results found
					</p>
				{:else}
					{#each groups as group}
						<div class="px-3 pb-1 pt-2">
							<p
								class="px-2 text-[10px] font-medium uppercase tracking-wider text-[var(--muted-foreground)]"
							>
								{group.name}
							</p>
						</div>
						{#each group.items as item, i}
							{@const globalIdx = filtered.indexOf(item)}
							<button
								onclick={item.action}
								onmouseenter={() => (selectedIndex = globalIdx)}
								class="flex w-full items-center gap-3 px-5 py-2 text-left text-sm transition-colors
									{globalIdx === selectedIndex
									? 'bg-[var(--secondary)] text-[var(--foreground)]'
									: 'text-[var(--muted-foreground)] hover:bg-[var(--secondary)]'}"
							>
								<span
									class="flex h-5 w-5 items-center justify-center rounded bg-[var(--secondary)] font-mono text-[10px] text-[var(--muted-foreground)]"
									>{item.icon}</span
								>
								{item.label}
							</button>
						{/each}
					{/each}
				{/if}
			</div>

			<!-- Footer -->
			<div
				class="flex flex-wrap items-center justify-between gap-x-3 gap-y-1 border-t border-[var(--border)] px-4 py-2 text-[10px] text-[var(--muted-foreground)]"
			>
				<div class="flex flex-wrap items-center gap-2">
					<span><kbd class="rounded border border-[var(--border)] px-1">↑↓</kbd> navigate</span>
					<span><kbd class="rounded border border-[var(--border)] px-1">↵</kbd> select</span>
					<span
						><kbd class="rounded border border-[var(--border)] px-1">⌘J</kbd> context ·
						<kbd class="rounded border border-[var(--border)] px-1">⌘`</kbd> bottom</span
					>
					<span
						><kbd class="rounded border border-[var(--border)] px-1">⌥1–9</kbd> depts</span
					>
				</div>
				<span><kbd class="rounded border border-[var(--border)] px-1">⌘K</kbd> toggle</span>
			</div>
		</div>
	</div>
{/if}
