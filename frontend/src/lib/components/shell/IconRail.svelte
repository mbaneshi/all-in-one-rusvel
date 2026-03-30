<script lang="ts">
	import { onMount } from 'svelte';
	import { browser } from '$app/environment';
	import { goto } from '$app/navigation';
	import { page } from '$app/state';
	import {
		MessageSquare,
		LayoutDashboard,
		Database,
		GitBranch,
		Settings,
		ClipboardCheck,
		ClipboardList,
		FileStack,
		TerminalSquare,
		Zap,
		PanelLeftClose,
		PanelLeftOpen,
		ChevronDown,
		Building2
	} from 'lucide-svelte';
	import { get } from 'svelte/store';
	import {
		departments,
		pendingApprovalCount,
		commandPaletteOpen,
		sidebarOpen,
		iconRailDeptsCollapsed
	} from '$lib/stores';
	import { deptHref } from '$lib/api';
	import type { DepartmentDef } from '$lib/api';
	import DeptIcon from '$lib/components/DeptIcon.svelte';

	let deptList: DepartmentDef[] = $state([]);
	departments.subscribe((v) => (deptList = v));

	let expanded = $state(false);
	sidebarOpen.subscribe((v) => (expanded = v));

	let deptsCollapsed = $state(false);
	iconRailDeptsCollapsed.subscribe((v) => (deptsCollapsed = v));

	function toggleDeptsSection() {
		iconRailDeptsCollapsed.update((v) => !v);
	}

	function toggle() {
		sidebarOpen.update((v) => !v);
	}

	const globalLinks = [
		{ href: '/', label: 'Dashboard', icon: LayoutDashboard, tour: 'nav-dashboard' },
		{ href: '/chat', label: 'Chat', icon: MessageSquare, tour: 'nav-chat' },
		{ href: '/tasks', label: 'Active tasks', icon: ClipboardList, tour: '' },
		{ href: '/artifacts', label: 'Artifacts', icon: FileStack, tour: '' },
		{ href: '/approvals', label: 'Approvals', icon: ClipboardCheck, tour: '' },
		{ href: '/database/schema', label: 'Database', icon: Database, tour: '' },
		{ href: '/automations', label: 'Automations', icon: Zap, tour: '' },
		{ href: '/flows', label: 'Flows', icon: GitBranch, tour: '' },
		{ href: '/terminal', label: 'Terminal', icon: TerminalSquare, tour: '' }
	] as const;

	function linkActive(href: string): boolean {
		const p = page.url.pathname;
		if (href === '/') return p === '/';
		return p === href || p.startsWith(`${href}/`);
	}

	function deptActive(id: string): boolean {
		const p = page.url.pathname;
		return p === `/dept/${id}` || p.startsWith(`/dept/${id}/`);
	}

	onMount(() => {
		let unsubs: Array<() => void> = [];
		if (browser) {
			try {
				if (localStorage.getItem('rusvel-sidebar-expanded') === '1') {
					sidebarOpen.set(true);
				}
				if (localStorage.getItem('rusvel-icon-rail-depts-collapsed') === '1') {
					iconRailDeptsCollapsed.set(true);
				}
			} catch {
				/* ignore */
			}
			const unsubSidebar = sidebarOpen.subscribe((open) => {
				try {
					localStorage.setItem('rusvel-sidebar-expanded', open ? '1' : '0');
				} catch {
					/* ignore */
				}
			});
			const unsubDepts = iconRailDeptsCollapsed.subscribe((c) => {
				try {
					localStorage.setItem('rusvel-icon-rail-depts-collapsed', c ? '1' : '0');
				} catch {
					/* ignore */
				}
			});
			unsubs.push(unsubSidebar, unsubDepts);
		}

		const onKey = (e: KeyboardEvent) => {
			if (!e.altKey || e.metaKey || e.ctrlKey || e.shiftKey) return;
			if (get(commandPaletteOpen)) return;
			const el = e.target as HTMLElement | null;
			if (el?.closest('input, textarea, select, [contenteditable="true"]')) return;
			const k = e.key;
			if (k < '1' || k > '9') return;
			const idx = Number.parseInt(k, 10) - 1;
			const d = deptList[idx];
			if (!d) return;
			e.preventDefault();
			goto(deptHref(d.id));
		};
		window.addEventListener('keydown', onKey);
		return () => {
			for (const u of unsubs) u();
			window.removeEventListener('keydown', onKey);
		};
	});
</script>

<aside
	class="flex shrink-0 flex-col border-r border-border bg-sidebar py-2 transition-[width] duration-200 ease-in-out
		{expanded ? 'w-48' : 'w-12'}"
	aria-label="Primary"
>
	<!-- Toggle button -->
	<button
		onclick={toggle}
		title={expanded ? 'Collapse sidebar' : 'Expand sidebar'}
		class="mx-auto mb-1 flex h-8 shrink-0 items-center justify-center rounded-md text-muted-foreground transition-colors hover:bg-sidebar-accent hover:text-sidebar-accent-foreground
			{expanded ? 'w-[calc(100%-0.5rem)] px-2' : 'w-8'}"
	>
		{#if expanded}
			<PanelLeftClose size={16} strokeWidth={1.75} />
			<span class="ml-2 truncate text-xs">Collapse</span>
		{:else}
			<PanelLeftOpen size={16} strokeWidth={1.75} />
		{/if}
	</button>

	<!-- Global links -->
	<div class="flex flex-col gap-0.5 {expanded ? 'px-1.5' : 'items-center'}">
		{#each globalLinks as item}
			{@const Icon = item.icon}
			{@const active = linkActive(item.href)}
			<a
				href={item.href}
				data-tour={item.tour || undefined}
				title={expanded ? undefined : item.label}
				class="relative flex h-9 shrink-0 items-center rounded-md transition-colors
					{expanded ? 'gap-2.5 px-2.5' : 'w-9 justify-center'}
					{active
					? 'bg-sidebar-primary/20 text-sidebar-primary'
					: 'text-muted-foreground hover:bg-sidebar-accent hover:text-sidebar-accent-foreground'}"
			>
				<Icon size={18} strokeWidth={1.75} class="shrink-0" />
				{#if expanded}
					<span class="truncate text-xs font-medium">{item.label}</span>
				{/if}
				{#if item.href === '/approvals' && $pendingApprovalCount > 0}
					<span
						class="absolute {expanded ? 'right-1.5 top-1' : 'right-0.5 top-0.5'} flex h-3.5 min-w-3.5 items-center justify-center rounded-full bg-destructive px-0.5 text-[8px] font-bold leading-none text-destructive-foreground"
					>
						{$pendingApprovalCount > 99 ? '99+' : $pendingApprovalCount}
					</span>
				{/if}
			</a>
		{/each}
	</div>

	<div class="my-1 {expanded ? 'mx-2.5' : 'mx-auto w-7'} h-px shrink-0 bg-border" aria-hidden="true"></div>

	{#if expanded}
		<button
			type="button"
			onclick={toggleDeptsSection}
			class="mx-1.5 mb-0.5 flex w-[calc(100%-0.75rem)] shrink-0 items-center gap-2 rounded-md px-2 py-1.5 text-left text-[10px] font-semibold uppercase tracking-wide text-muted-foreground transition-colors hover:bg-sidebar-accent hover:text-sidebar-accent-foreground"
			title={deptsCollapsed ? 'Expand department list' : 'Collapse department list (less scrolling)'}
		>
			<ChevronDown
				size={14}
				strokeWidth={2}
				class="shrink-0 transition-transform duration-200 {deptsCollapsed ? '-rotate-90' : ''}"
			/>
			<Building2 size={14} strokeWidth={1.75} class="shrink-0 opacity-80" />
			<span class="min-w-0 flex-1 truncate">Departments · {deptList.length}</span>
		</button>
	{/if}

	<!-- Department links (icon-only when sidebar collapsed; full list when expanded unless section collapsed) -->
	<div class="flex min-h-0 w-full flex-1 flex-col gap-0.5 overflow-y-auto overflow-x-hidden {expanded ? 'px-1.5' : 'items-center px-0.5'}">
		{#if !expanded || !deptsCollapsed}
			{#each deptList as d, i}
				<a
					href={deptHref(d.id)}
					data-tour={d.id === 'forge' ? 'nav-forge' : undefined}
					title={expanded ? undefined : (i < 9 ? `${d.name} (Alt+${i + 1})` : d.name)}
					class="flex h-9 shrink-0 items-center rounded-md transition-colors
						{expanded ? 'gap-2.5 px-2.5' : 'w-9 justify-center'}
						{deptActive(d.id)
						? 'bg-sidebar-primary/20 text-sidebar-primary'
						: 'text-muted-foreground hover:bg-sidebar-accent hover:text-sidebar-accent-foreground'}"
				>
					<DeptIcon deptId={d.id} size={18} strokeWidth={1.75} class="shrink-0" />
					{#if expanded}
						<span class="truncate text-xs font-medium">{d.name}</span>
						{#if i < 9}
							<kbd class="ml-auto shrink-0 rounded border border-border px-1 text-[9px] text-muted-foreground"
								>Alt+{i + 1}</kbd>
						{/if}
					{/if}
				</a>
			{/each}
		{/if}
	</div>

	<!-- Settings -->
	<a
		href="/settings"
		title={expanded ? undefined : 'Settings'}
		class="mt-auto flex h-9 shrink-0 items-center rounded-md text-muted-foreground transition-colors hover:bg-sidebar-accent hover:text-sidebar-accent-foreground
			{expanded ? 'mx-1.5 gap-2.5 px-2.5' : 'mx-auto w-9 justify-center'}
			{linkActive('/settings') ? 'bg-sidebar-primary/20 text-sidebar-primary' : ''}"
	>
		<Settings size={18} strokeWidth={1.75} class="shrink-0" />
		{#if expanded}
			<span class="truncate text-xs font-medium">Settings</span>
		{/if}
	</a>
</aside>
