<script lang="ts">
	import { onMount } from 'svelte';
	import { toast } from 'svelte-sonner';
	import { panelOpen, panelWidth, pendingCommand } from '$lib/stores';
	import {
		getAgents,
		getSkills,
		createSkill,
		deleteSkill,
		getRules,
		createRule,
		updateRule,
		deleteRule,
		getMcpServers,
		createMcpServer,
		deleteMcpServer,
		getHooks,
		createHook,
		updateHook,
		deleteHook,
		getDeptEvents,
		getDeptConfig,
		updateDeptConfig,
		getWorkflows
	} from '$lib/api';
	import type {
		Agent,
		Skill,
		Rule,
		McpServer,
		Hook,
		Event,
		DepartmentConfig,
		Workflow
	} from '$lib/api';
	import DeptHelpTooltip from '$lib/components/onboarding/DeptHelpTooltip.svelte';
	import DepartmentPanelAgentsTab from '$lib/components/chat/DepartmentPanelAgentsTab.svelte';
	import DepartmentPanelWorkflowsTab from '$lib/components/chat/DepartmentPanelWorkflowsTab.svelte';

	let {
		dept,
		title,
		icon,
		color,
		quickActions = [],
		tabs = ['actions', 'agents', 'workflows', 'skills', 'rules', 'mcp', 'hooks', 'dirs', 'events'],
		helpDescription = '',
		helpPrompts = []
	}: {
		dept: string;
		title: string;
		icon: string;
		color: string;
		quickActions: { label: string; prompt: string }[];
		tabs?: string[];
		helpDescription?: string;
		helpPrompts?: string[];
	} = $props();

	let activeTab = $state('actions');
	let agents: Agent[] = $state([]);
	let skills: Skill[] = $state([]);
	let rules: Rule[] = $state([]);
	let mcpServers: McpServer[] = $state([]);
	let hooks: Hook[] = $state([]);
	let events: Event[] = $state([]);
	let config: DepartmentConfig | null = $state(null);

	// Create forms
	let showCreateSkill = $state(false);
	let newSkillName = $state('');
	let newSkillDesc = $state('');
	let newSkillPrompt = $state('');

	let showCreateRule = $state(false);
	let newRuleName = $state('');
	let newRuleContent = $state('');

	let showCreateMcp = $state(false);
	let newMcpName = $state('');
	let newMcpType = $state('stdio');
	let newMcpCommand = $state('');

	let showCreateHook = $state(false);
	let newHookName = $state('');
	let newHookEvent = $state('PostToolUse');
	let newHookAction = $state('');

	// Capability Engine
	let showCapability = $state(false);
	let capabilityInput = $state('');

	let workflows: Workflow[] = $state([]);

	onMount(() => {
		loadAgents();
		loadSkills();
		loadRules();
		loadMcp();
		loadHooks();
		loadWorkflows();
		loadConfig();
	});

	async function loadAgents() {
		try {
			agents = await getAgents(dept);
		} catch {
			agents = [];
		}
	}
	async function loadSkills() {
		try {
			skills = await getSkills(dept);
		} catch {
			skills = [];
		}
	}
	async function loadRules() {
		try {
			rules = await getRules(dept);
		} catch {
			rules = [];
		}
	}
	async function loadMcp() {
		try {
			mcpServers = await getMcpServers(dept);
		} catch {
			mcpServers = [];
		}
	}
	async function loadHooks() {
		try {
			hooks = await getHooks(dept);
		} catch {
			hooks = [];
		}
	}
	async function loadEvents() {
		try {
			events = await getDeptEvents(dept);
		} catch {
			events = [];
		}
	}
	async function loadWorkflows() {
		try {
			workflows = await getWorkflows();
		} catch {
			workflows = [];
		}
	}
	async function loadConfig() {
		try {
			config = await getDeptConfig(dept);
		} catch {}
	}

	function sendQuickAction(prompt: string) {
		pendingCommand.set({ prompt });
	}

	async function handleCreateSkill() {
		if (!newSkillName.trim()) return;
		try {
			await createSkill({
				id: '',
				name: newSkillName.trim(),
				description: newSkillDesc,
				prompt_template: newSkillPrompt,
				metadata: { engine: dept }
			});
			newSkillName = '';
			newSkillDesc = '';
			newSkillPrompt = '';
			showCreateSkill = false;
			await loadSkills();
			toast.success('Skill created');
		} catch (e) {
			toast.error(`Failed to create skill: ${e instanceof Error ? e.message : e}`);
		}
	}

	async function handleDeleteSkill(id: string) {
		try {
			await deleteSkill(id);
			await loadSkills();
			toast.success('Skill deleted');
		} catch (e) {
			toast.error(`Failed to delete skill: ${e instanceof Error ? e.message : e}`);
		}
	}

	async function handleCreateRule() {
		if (!newRuleName.trim()) return;
		try {
			await createRule({
				id: '',
				name: newRuleName.trim(),
				content: newRuleContent,
				enabled: true,
				metadata: { engine: dept }
			});
			newRuleName = '';
			newRuleContent = '';
			showCreateRule = false;
			await loadRules();
			toast.success('Rule created');
		} catch (e) {
			toast.error(`Failed to create rule: ${e instanceof Error ? e.message : e}`);
		}
	}

	async function handleToggleRule(rule: Rule) {
		try {
			await updateRule(rule.id, { ...rule, enabled: !rule.enabled });
			await loadRules();
		} catch (e) {
			toast.error(`Failed to update rule: ${e instanceof Error ? e.message : e}`);
		}
	}

	async function handleDeleteRule(id: string) {
		try {
			await deleteRule(id);
			await loadRules();
			toast.success('Rule deleted');
		} catch (e) {
			toast.error(`Failed to delete rule: ${e instanceof Error ? e.message : e}`);
		}
	}

	async function handleCreateMcp() {
		if (!newMcpName.trim()) return;
		try {
			await createMcpServer({
				id: '',
				name: newMcpName.trim(),
				description: '',
				server_type: newMcpType,
				command: newMcpCommand || null,
				args: [],
				url: null,
				env: {},
				enabled: true,
				metadata: { engine: dept }
			});
			newMcpName = '';
			newMcpCommand = '';
			showCreateMcp = false;
			await loadMcp();
			toast.success('MCP server added');
		} catch (e) {
			toast.error(`Failed to add MCP server: ${e instanceof Error ? e.message : e}`);
		}
	}

	async function handleDeleteMcp(id: string) {
		try {
			await deleteMcpServer(id);
			await loadMcp();
			toast.success('MCP server removed');
		} catch (e) {
			toast.error(`Failed to remove MCP server: ${e instanceof Error ? e.message : e}`);
		}
	}

	async function handleCreateHook() {
		if (!newHookName.trim()) return;
		try {
			await createHook({
				id: '',
				name: newHookName.trim(),
				event: newHookEvent,
				matcher: '',
				hook_type: 'command',
				action: newHookAction,
				enabled: true,
				metadata: { engine: dept }
			});
			newHookName = '';
			newHookAction = '';
			showCreateHook = false;
			await loadHooks();
			toast.success('Hook created');
		} catch (e) {
			toast.error(`Failed to create hook: ${e instanceof Error ? e.message : e}`);
		}
	}

	async function handleToggleHook(hook: Hook) {
		try {
			await updateHook(hook.id, { ...hook, enabled: !hook.enabled });
			await loadHooks();
		} catch (e) {
			toast.error(`Failed to update hook: ${e instanceof Error ? e.message : e}`);
		}
	}
	async function handleDeleteHook(id: string) {
		try {
			await deleteHook(id);
			await loadHooks();
			toast.success('Hook deleted');
		} catch (e) {
			toast.error(`Failed to delete hook: ${e instanceof Error ? e.message : e}`);
		}
	}

	async function addDir() {
		if (!config) return;
		const dir = prompt('Add directory path:');
		if (dir && !config.add_dirs.includes(dir)) {
			config.add_dirs = [...config.add_dirs, dir];
			config = await updateDeptConfig(dept, config);
		}
	}

	async function removeDir(dir: string) {
		if (!config) return;
		config.add_dirs = config.add_dirs.filter((d) => d !== dir);
		config = await updateDeptConfig(dept, config);
	}

	// Static color map — Tailwind purges dynamic classes like `bg-${color}-600/20`
	const colorClasses: Record<
		string,
		{
			iconBg: string;
			iconText: string;
			hoverBgStrong: string;
			tabActive: string;
			tabBorder: string;
			borderLight: string;
			bgSubtle: string;
			hoverBgSubtle: string;
			hoverBg: string;
			hoverBorder: string;
			groupHoverText: string;
			badge: string;
			badgeText: string;
			text300: string;
			text400: string;
			button: string;
			buttonHover: string;
			buttonSemi: string;
		}
	> = {
		emerald: {
			iconBg: 'bg-emerald-600/20',
			iconText: 'text-emerald-400',
			hoverBgStrong: 'hover:bg-emerald-600/40',
			tabActive: 'text-emerald-300',
			tabBorder: 'border-emerald-500',
			borderLight: 'border-emerald-500/30',
			bgSubtle: 'bg-emerald-900/10',
			hoverBgSubtle: 'hover:bg-emerald-900/20',
			hoverBg: 'hover:bg-emerald-900/15',
			hoverBorder: 'hover:border-emerald-500/30',
			groupHoverText: 'group-hover:text-emerald-300',
			badge: 'bg-emerald-900/30',
			badgeText: 'text-emerald-400',
			text300: 'text-emerald-300',
			text400: 'text-emerald-400',
			button: 'bg-emerald-600',
			buttonHover: 'hover:bg-emerald-500',
			buttonSemi: 'bg-emerald-600/80'
		},
		purple: {
			iconBg: 'bg-purple-600/20',
			iconText: 'text-purple-400',
			hoverBgStrong: 'hover:bg-purple-600/40',
			tabActive: 'text-purple-300',
			tabBorder: 'border-purple-500',
			borderLight: 'border-purple-500/30',
			bgSubtle: 'bg-purple-900/10',
			hoverBgSubtle: 'hover:bg-purple-900/20',
			hoverBg: 'hover:bg-purple-900/15',
			hoverBorder: 'hover:border-purple-500/30',
			groupHoverText: 'group-hover:text-purple-300',
			badge: 'bg-purple-900/30',
			badgeText: 'text-purple-400',
			text300: 'text-purple-300',
			text400: 'text-purple-400',
			button: 'bg-purple-600',
			buttonHover: 'hover:bg-purple-500',
			buttonSemi: 'bg-purple-600/80'
		},
		amber: {
			iconBg: 'bg-amber-600/20',
			iconText: 'text-amber-400',
			hoverBgStrong: 'hover:bg-amber-600/40',
			tabActive: 'text-amber-300',
			tabBorder: 'border-amber-500',
			borderLight: 'border-amber-500/30',
			bgSubtle: 'bg-amber-900/10',
			hoverBgSubtle: 'hover:bg-amber-900/20',
			hoverBg: 'hover:bg-amber-900/15',
			hoverBorder: 'hover:border-amber-500/30',
			groupHoverText: 'group-hover:text-amber-300',
			badge: 'bg-amber-900/30',
			badgeText: 'text-amber-400',
			text300: 'text-amber-300',
			text400: 'text-amber-400',
			button: 'bg-amber-600',
			buttonHover: 'hover:bg-amber-500',
			buttonSemi: 'bg-amber-600/80'
		},
		cyan: {
			iconBg: 'bg-cyan-600/20',
			iconText: 'text-cyan-400',
			hoverBgStrong: 'hover:bg-cyan-600/40',
			tabActive: 'text-cyan-300',
			tabBorder: 'border-cyan-500',
			borderLight: 'border-cyan-500/30',
			bgSubtle: 'bg-cyan-900/10',
			hoverBgSubtle: 'hover:bg-cyan-900/20',
			hoverBg: 'hover:bg-cyan-900/15',
			hoverBorder: 'hover:border-cyan-500/30',
			groupHoverText: 'group-hover:text-cyan-300',
			badge: 'bg-cyan-900/30',
			badgeText: 'text-cyan-400',
			text300: 'text-cyan-300',
			text400: 'text-cyan-400',
			button: 'bg-cyan-600',
			buttonHover: 'hover:bg-cyan-500',
			buttonSemi: 'bg-cyan-600/80'
		},
		indigo: {
			iconBg: 'bg-indigo-600/20',
			iconText: 'text-indigo-400',
			hoverBgStrong: 'hover:bg-indigo-600/40',
			tabActive: 'text-indigo-300',
			tabBorder: 'border-indigo-500',
			borderLight: 'border-indigo-500/30',
			bgSubtle: 'bg-indigo-900/10',
			hoverBgSubtle: 'hover:bg-indigo-900/20',
			hoverBg: 'hover:bg-indigo-900/15',
			hoverBorder: 'hover:border-indigo-500/30',
			groupHoverText: 'group-hover:text-indigo-300',
			badge: 'bg-indigo-900/30',
			badgeText: 'text-indigo-400',
			text300: 'text-indigo-300',
			text400: 'text-indigo-400',
			button: 'bg-indigo-600',
			buttonHover: 'hover:bg-indigo-500',
			buttonSemi: 'bg-indigo-600/80'
		},
		rose: {
			iconBg: 'bg-rose-600/20',
			iconText: 'text-rose-400',
			hoverBgStrong: 'hover:bg-rose-600/40',
			tabActive: 'text-rose-300',
			tabBorder: 'border-rose-500',
			borderLight: 'border-rose-500/30',
			bgSubtle: 'bg-rose-900/10',
			hoverBgSubtle: 'hover:bg-rose-900/20',
			hoverBg: 'hover:bg-rose-900/15',
			hoverBorder: 'hover:border-rose-500/30',
			groupHoverText: 'group-hover:text-rose-300',
			badge: 'bg-rose-900/30',
			badgeText: 'text-rose-400',
			text300: 'text-rose-300',
			text400: 'text-rose-400',
			button: 'bg-rose-600',
			buttonHover: 'hover:bg-rose-500',
			buttonSemi: 'bg-rose-600/80'
		},
		sky: {
			iconBg: 'bg-sky-600/20',
			iconText: 'text-sky-400',
			hoverBgStrong: 'hover:bg-sky-600/40',
			tabActive: 'text-sky-300',
			tabBorder: 'border-sky-500',
			borderLight: 'border-sky-500/30',
			bgSubtle: 'bg-sky-900/10',
			hoverBgSubtle: 'hover:bg-sky-900/20',
			hoverBg: 'hover:bg-sky-900/15',
			hoverBorder: 'hover:border-sky-500/30',
			groupHoverText: 'group-hover:text-sky-300',
			badge: 'bg-sky-900/30',
			badgeText: 'text-sky-400',
			text300: 'text-sky-300',
			text400: 'text-sky-400',
			button: 'bg-sky-600',
			buttonHover: 'hover:bg-sky-500',
			buttonSemi: 'bg-sky-600/80'
		},
		orange: {
			iconBg: 'bg-orange-600/20',
			iconText: 'text-orange-400',
			hoverBgStrong: 'hover:bg-orange-600/40',
			tabActive: 'text-orange-300',
			tabBorder: 'border-orange-500',
			borderLight: 'border-orange-500/30',
			bgSubtle: 'bg-orange-900/10',
			hoverBgSubtle: 'hover:bg-orange-900/20',
			hoverBg: 'hover:bg-orange-900/15',
			hoverBorder: 'hover:border-orange-500/30',
			groupHoverText: 'group-hover:text-orange-300',
			badge: 'bg-orange-900/30',
			badgeText: 'text-orange-400',
			text300: 'text-orange-300',
			text400: 'text-orange-400',
			button: 'bg-orange-600',
			buttonHover: 'hover:bg-orange-500',
			buttonSemi: 'bg-orange-600/80'
		},
		lime: {
			iconBg: 'bg-lime-600/20',
			iconText: 'text-lime-400',
			hoverBgStrong: 'hover:bg-lime-600/40',
			tabActive: 'text-lime-300',
			tabBorder: 'border-lime-500',
			borderLight: 'border-lime-500/30',
			bgSubtle: 'bg-lime-900/10',
			hoverBgSubtle: 'hover:bg-lime-900/20',
			hoverBg: 'hover:bg-lime-900/15',
			hoverBorder: 'hover:border-lime-500/30',
			groupHoverText: 'group-hover:text-lime-300',
			badge: 'bg-lime-900/30',
			badgeText: 'text-lime-400',
			text300: 'text-lime-300',
			text400: 'text-lime-400',
			button: 'bg-lime-600',
			buttonHover: 'hover:bg-lime-500',
			buttonSemi: 'bg-lime-600/80'
		},
		pink: {
			iconBg: 'bg-pink-600/20',
			iconText: 'text-pink-400',
			hoverBgStrong: 'hover:bg-pink-600/40',
			tabActive: 'text-pink-300',
			tabBorder: 'border-pink-500',
			borderLight: 'border-pink-500/30',
			bgSubtle: 'bg-pink-900/10',
			hoverBgSubtle: 'hover:bg-pink-900/20',
			hoverBg: 'hover:bg-pink-900/15',
			hoverBorder: 'hover:border-pink-500/30',
			groupHoverText: 'group-hover:text-pink-300',
			badge: 'bg-pink-900/30',
			badgeText: 'text-pink-400',
			text300: 'text-pink-300',
			text400: 'text-pink-400',
			button: 'bg-pink-600',
			buttonHover: 'hover:bg-pink-500',
			buttonSemi: 'bg-pink-600/80'
		},
		teal: {
			iconBg: 'bg-teal-600/20',
			iconText: 'text-teal-400',
			hoverBgStrong: 'hover:bg-teal-600/40',
			tabActive: 'text-teal-300',
			tabBorder: 'border-teal-500',
			borderLight: 'border-teal-500/30',
			bgSubtle: 'bg-teal-900/10',
			hoverBgSubtle: 'hover:bg-teal-900/20',
			hoverBg: 'hover:bg-teal-900/15',
			hoverBorder: 'hover:border-teal-500/30',
			groupHoverText: 'group-hover:text-teal-300',
			badge: 'bg-teal-900/30',
			badgeText: 'text-teal-400',
			text300: 'text-teal-300',
			text400: 'text-teal-400',
			button: 'bg-teal-600',
			buttonHover: 'hover:bg-teal-500',
			buttonSemi: 'bg-teal-600/80'
		},
		violet: {
			iconBg: 'bg-violet-600/20',
			iconText: 'text-violet-400',
			hoverBgStrong: 'hover:bg-violet-600/40',
			tabActive: 'text-violet-300',
			tabBorder: 'border-violet-500',
			borderLight: 'border-violet-500/30',
			bgSubtle: 'bg-violet-900/10',
			hoverBgSubtle: 'hover:bg-violet-900/20',
			hoverBg: 'hover:bg-violet-900/15',
			hoverBorder: 'hover:border-violet-500/30',
			groupHoverText: 'group-hover:text-violet-300',
			badge: 'bg-violet-900/30',
			badgeText: 'text-violet-400',
			text300: 'text-violet-300',
			text400: 'text-violet-400',
			button: 'bg-violet-600',
			buttonHover: 'hover:bg-violet-500',
			buttonSemi: 'bg-violet-600/80'
		}
	};

	// Resolve once — falls back to indigo if color not in map
	const cc = $derived(colorClasses[color] ?? colorClasses.indigo);

	let isOpen = $state(true);
	let width = $state(288);
	let resizing = $state(false);

	panelOpen.subscribe((v) => (isOpen = v));
	panelWidth.subscribe((v) => (width = v));

	function togglePanel() {
		panelOpen.update((v) => !v);
	}

	function startResize(e: MouseEvent) {
		e.preventDefault();
		resizing = true;
		const startX = e.clientX;
		const startWidth = width;
		const onMove = (ev: MouseEvent) => {
			const delta = ev.clientX - startX;
			const newWidth = Math.max(200, Math.min(500, startWidth + delta));
			panelWidth.set(newWidth);
		};
		const onUp = () => {
			resizing = false;
			window.removeEventListener('mousemove', onMove);
			window.removeEventListener('mouseup', onUp);
		};
		window.addEventListener('mousemove', onMove);
		window.addEventListener('mouseup', onUp);
	}

	function formatTime(iso: string): string {
		try {
			return new Date(iso).toLocaleTimeString([], { hour: '2-digit', minute: '2-digit' });
		} catch {
			return iso;
		}
	}
</script>

{#if !isOpen}
	<!-- Collapsed: just a narrow strip with icon + expand button -->
	<div
		class="flex w-10 flex-shrink-0 flex-col items-center border-r border-[var(--border)] bg-[var(--card)] py-3 gap-2"
	>
		<button
			onclick={togglePanel}
			class="flex h-8 w-8 items-center justify-center rounded-lg {cc.iconBg} text-sm font-bold {cc.iconText} {cc.hoverBgStrong}"
			title="Expand {title}"
		>
			{icon}
		</button>
	</div>
{:else}
	<div
		class="flex flex-shrink-0 flex-col border-r border-[var(--border)] bg-[var(--card)] relative"
		class:select-none={resizing}
		style="width: {width}px"
	>
		<!-- Header -->
		<div class="border-b border-[var(--border)] px-4 py-3">
			<div class="flex items-center justify-between">
				<div class="flex items-center gap-2 min-w-0">
					<div
						class="flex h-8 w-8 flex-shrink-0 items-center justify-center rounded-lg {cc.iconBg} text-sm font-bold {cc.iconText}"
					>
						{icon}
					</div>
					<div class="min-w-0">
						<h2 class="text-sm font-semibold text-[var(--foreground)] truncate">{title}</h2>
						<p class="text-[10px] text-[var(--muted-foreground)]">{dept} department</p>
					</div>
				</div>
				<div class="flex items-center gap-1">
					{#if helpDescription}
						<DeptHelpTooltip {dept} description={helpDescription} prompts={helpPrompts} />
					{/if}
					<button
						onclick={togglePanel}
						class="rounded-md p-1 text-[var(--muted-foreground)] hover:bg-[var(--secondary)] hover:text-[var(--foreground)]"
						title="Collapse panel"
					>
						<svg
							class="h-4 w-4"
							viewBox="0 0 16 16"
							fill="none"
							stroke="currentColor"
							stroke-width="1.5"><path d="M10 3L5 8l5 5" /></svg
						>
					</button>
				</div>
			</div>
		</div>

		<!-- Tabs -->
		<div class="flex border-b border-[var(--border)] overflow-x-auto">
			{#each [{ id: 'actions', label: 'Actions' }, { id: 'agents', label: `Agents (${agents.length})` }, { id: 'workflows', label: `Flows (${workflows.length})` }, { id: 'skills', label: `Skills (${skills.length})` }, { id: 'rules', label: `Rules (${rules.length})` }, { id: 'mcp', label: `MCP (${mcpServers.length})` }, { id: 'hooks', label: `Hooks (${hooks.length})` }, { id: 'projects', label: 'Dirs' }, { id: 'events', label: 'Events' }].filter((t) => tabs.includes(t.id) || (t.id === 'projects' && tabs.includes('dirs'))) as tab}
				<button
					onclick={() => {
						activeTab = tab.id;
						if (tab.id === 'events') loadEvents();
					}}
					class="flex-shrink-0 px-2.5 py-2 text-[10px] font-medium transition-colors border-b-2
					{activeTab === tab.id
						? `${cc.tabBorder} ${cc.tabActive}`
						: 'border-transparent text-[var(--muted-foreground)] hover:text-[var(--foreground)]'}"
				>
					{tab.label}
				</button>
			{/each}
		</div>

		<!-- Content -->
		<div class="flex-1 overflow-y-auto">
			<!-- ACTIONS -->
			{#if activeTab === 'actions'}
				<div class="p-3 space-y-2">
					<!-- Build Capability -->
					<button
						onclick={() => (showCapability = !showCapability)}
						class="w-full rounded-lg border border-dashed {cc.borderLight} {cc.bgSubtle} px-3 py-2 text-left transition-colors {cc.hoverBgSubtle}"
					>
						<p class="text-xs font-medium {cc.text300}">Build Capability</p>
						<p class="text-[10px] text-[var(--muted-foreground)]">
							Describe what you need — AI discovers & installs
						</p>
					</button>
					{#if showCapability}
						<div class="rounded-lg bg-[var(--secondary)] p-3 space-y-2">
							<textarea
								bind:value={capabilityInput}
								placeholder="e.g. I need to scrape job postings and score them"
								rows="3"
								class="w-full rounded-md border border-[var(--border)] bg-[var(--background)] px-2 py-1 text-xs text-[var(--foreground)] focus:outline-none resize-none"
							></textarea>
							<button
								onclick={() => {
									if (capabilityInput.trim()) {
										sendQuickAction('!capability ' + capabilityInput.trim());
										capabilityInput = '';
										showCapability = false;
									}
								}}
								disabled={!capabilityInput.trim()}
								class="w-full rounded-md {cc.button} py-1 text-xs font-medium text-white {cc.buttonHover} disabled:opacity-40 disabled:cursor-not-allowed"
								>Search & Build</button
							>
						</div>
					{/if}

					<!-- Quick Actions -->
					{#each quickActions as action}
						<button
							onclick={() => sendQuickAction(action.prompt)}
							class="w-full rounded-lg bg-[var(--secondary)] px-3 py-2 text-left transition-colors {cc.hoverBg} group"
						>
							<p class="text-xs font-medium text-[var(--foreground)] {cc.groupHoverText}">
								{action.label}
							</p>
						</button>
					{/each}
				</div>

				<!-- AGENTS -->
			{:else if activeTab === 'agents'}
				<DepartmentPanelAgentsTab bind:agents {dept} {cc} />

				<!-- WORKFLOWS -->
			{:else if activeTab === 'workflows'}
				<DepartmentPanelWorkflowsTab bind:workflows {dept} {cc} {agents} />

				<!-- SKILLS -->
			{:else if activeTab === 'skills'}
				<div class="p-3 space-y-2">
					<button
						onclick={() => (showCreateSkill = !showCreateSkill)}
						class="w-full rounded-lg border border-dashed border-[var(--border)] py-1.5 text-xs text-[var(--muted-foreground)] {cc.hoverBorder} hover:text-[var(--foreground)]"
					>
						+ New Skill
					</button>
					{#if showCreateSkill}
						<div class="rounded-lg bg-[var(--secondary)] p-3 space-y-2">
							<input
								bind:value={newSkillName}
								placeholder="Skill name (e.g. /wire-engine)"
								class="w-full rounded-md border border-[var(--border)] bg-[var(--background)] px-2 py-1 text-xs text-[var(--foreground)] focus:outline-none"
							/>
							<input
								bind:value={newSkillDesc}
								placeholder="Description"
								class="w-full rounded-md border border-[var(--border)] bg-[var(--background)] px-2 py-1 text-xs text-[var(--foreground)] focus:outline-none"
							/>
							<textarea
								bind:value={newSkillPrompt}
								placeholder="Prompt template"
								rows="3"
								class="w-full rounded-md border border-[var(--border)] bg-[var(--background)] px-2 py-1 text-xs text-[var(--foreground)] focus:outline-none resize-none"
							></textarea>
							<button
								onclick={handleCreateSkill}
								class="w-full rounded-md {cc.button} py-1 text-xs font-medium text-white {cc.buttonHover}"
								>Create</button
							>
						</div>
					{/if}
					{#each skills as skill}
						<div
							class="rounded-lg bg-[var(--secondary)] p-2.5 transition-colors {cc.hoverBg} group cursor-pointer"
							role="button"
							tabindex="0"
							onclick={() => sendQuickAction('/' + skill.name.toLowerCase().replace(/ /g, '-'))}
							onkeydown={(e) => {
								if (e.key === 'Enter')
									sendQuickAction('/' + skill.name.toLowerCase().replace(/ /g, '-'));
							}}
						>
							<div class="flex items-center justify-between">
								<span class="text-xs font-mono font-medium {cc.text400}">{skill.name}</span>
								<button
									onclick={(e) => {
										e.stopPropagation();
										handleDeleteSkill(skill.id);
									}}
									class="hidden group-hover:block text-[var(--muted-foreground)] hover:text-danger-400 text-[10px]"
									>x</button
								>
							</div>
							<p class="text-[10px] text-[var(--muted-foreground)]">{skill.description}</p>
						</div>
					{/each}
					{#if skills.length === 0 && !showCreateSkill}
						<p class="text-center text-[10px] text-[var(--muted-foreground)] py-2">
							No skills. Create one above.
						</p>
					{/if}
				</div>

				<!-- RULES -->
			{:else if activeTab === 'rules'}
				<div class="p-3 space-y-2">
					<button
						onclick={() => (showCreateRule = !showCreateRule)}
						class="w-full rounded-lg border border-dashed border-[var(--border)] py-1.5 text-xs text-[var(--muted-foreground)] {cc.hoverBorder} hover:text-[var(--foreground)]"
					>
						+ New Rule
					</button>
					{#if showCreateRule}
						<div class="rounded-lg bg-[var(--secondary)] p-3 space-y-2">
							<input
								bind:value={newRuleName}
								placeholder="Rule name"
								class="w-full rounded-md border border-[var(--border)] bg-[var(--background)] px-2 py-1 text-xs text-[var(--foreground)] focus:outline-none"
							/>
							<textarea
								bind:value={newRuleContent}
								placeholder="Rule content (injected into system prompt)"
								rows="3"
								class="w-full rounded-md border border-[var(--border)] bg-[var(--background)] px-2 py-1 text-xs text-[var(--foreground)] focus:outline-none resize-none"
							></textarea>
							<button
								onclick={handleCreateRule}
								class="w-full rounded-md {cc.button} py-1 text-xs font-medium text-white {cc.buttonHover}"
								>Create</button
							>
						</div>
					{/if}
					{#each rules as rule}
						<div class="rounded-lg bg-[var(--secondary)] p-2.5 group">
							<div class="flex items-center justify-between mb-1">
								<span
									class="text-xs font-medium text-[var(--foreground)] {!rule.enabled
										? 'line-through opacity-50'
										: ''}">{rule.name}</span
								>
								<div class="flex items-center gap-1">
									<button
										onclick={() => handleToggleRule(rule)}
										class="rounded px-1.5 py-0.5 text-[9px] {rule.enabled
											? `${cc.badge} ${cc.text400}`
											: 'bg-[var(--card)] text-[var(--muted-foreground)]'}"
									>
										{rule.enabled ? 'on' : 'off'}
									</button>
									<button
										onclick={() => handleDeleteRule(rule.id)}
										class="hidden group-hover:block text-[var(--muted-foreground)] hover:text-danger-400 text-[10px]"
										>x</button
									>
								</div>
							</div>
							<p
								class="text-[10px] text-[var(--muted-foreground)] {!rule.enabled
									? 'opacity-50'
									: ''}"
							>
								{rule.content.slice(0, 80)}{rule.content.length > 80 ? '...' : ''}
							</p>
						</div>
					{/each}
					{#if rules.length === 0 && !showCreateRule}
						<p class="text-center text-[10px] text-[var(--muted-foreground)] py-2">
							No rules. Rules get injected into system prompts.
						</p>
					{/if}
				</div>

				<!-- MCP SERVERS -->
			{:else if activeTab === 'mcp'}
				<div class="p-3 space-y-2">
					<button
						onclick={() => (showCreateMcp = !showCreateMcp)}
						class="w-full rounded-lg border border-dashed border-[var(--border)] py-1.5 text-xs text-[var(--muted-foreground)] hover:text-[var(--foreground)]"
					>
						+ Add MCP Server
					</button>
					{#if showCreateMcp}
						<div class="rounded-lg bg-[var(--secondary)] p-3 space-y-2">
							<input
								bind:value={newMcpName}
								placeholder="Server name"
								class="w-full rounded-md border border-[var(--border)] bg-[var(--background)] px-2 py-1 text-xs text-[var(--foreground)] focus:outline-none"
							/>
							<select
								bind:value={newMcpType}
								class="w-full rounded-md border border-[var(--border)] bg-[var(--background)] px-2 py-1 text-xs text-[var(--foreground)]"
							>
								<option value="stdio">stdio</option>
								<option value="http">HTTP</option>
								<option value="sse">SSE</option>
								<option value="ws">WebSocket</option>
							</select>
							<input
								bind:value={newMcpCommand}
								placeholder="Command (e.g. npx @server/mcp)"
								class="w-full rounded-md border border-[var(--border)] bg-[var(--background)] px-2 py-1 text-xs text-[var(--foreground)] focus:outline-none"
							/>
							<button
								onclick={handleCreateMcp}
								class="w-full rounded-md bg-[var(--primary)] py-1 text-xs font-medium text-white"
								>Create</button
							>
						</div>
					{/if}
					{#each mcpServers as server}
						<div class="rounded-lg bg-[var(--secondary)] p-2.5 group">
							<div class="flex items-center justify-between mb-1">
								<span class="text-xs font-medium text-[var(--foreground)]">{server.name}</span>
								<div class="flex items-center gap-1">
									<span
										class="rounded bg-[var(--card)] px-1.5 py-0.5 text-[9px] text-[var(--muted-foreground)]"
										>{server.server_type}</span
									>
									<button
										onclick={() => handleDeleteMcp(server.id)}
										class="hidden group-hover:block text-[var(--muted-foreground)] hover:text-danger-400 text-[10px]"
										>x</button
									>
								</div>
							</div>
							<p class="text-[10px] font-mono text-[var(--muted-foreground)]">
								{server.command || server.url || '—'}
							</p>
						</div>
					{/each}
					{#if mcpServers.length === 0 && !showCreateMcp}
						<p class="text-center text-[10px] text-[var(--muted-foreground)] py-2">
							No MCP servers. Add one to extend capabilities.
						</p>
					{/if}
				</div>

				<!-- HOOKS -->
			{:else if activeTab === 'hooks'}
				<div class="p-3 space-y-2">
					<button
						onclick={() => (showCreateHook = !showCreateHook)}
						class="w-full rounded-lg border border-dashed border-[var(--border)] py-1.5 text-xs text-[var(--muted-foreground)] hover:text-[var(--foreground)]"
					>
						+ Add Hook
					</button>
					{#if showCreateHook}
						<div class="rounded-lg bg-[var(--secondary)] p-3 space-y-2">
							<input
								bind:value={newHookName}
								placeholder="Hook name"
								class="w-full rounded-md border border-[var(--border)] bg-[var(--background)] px-2 py-1 text-xs text-[var(--foreground)] focus:outline-none"
							/>
							<select
								bind:value={newHookEvent}
								class="w-full rounded-md border border-[var(--border)] bg-[var(--background)] px-2 py-1 text-xs text-[var(--foreground)]"
							>
								<option value="PreToolUse">PreToolUse</option>
								<option value="PostToolUse">PostToolUse</option>
								<option value="SessionStart">SessionStart</option>
								<option value="Stop">Stop</option>
								<option value="TaskCompleted">TaskCompleted</option>
								<option value="UserPromptSubmit">UserPromptSubmit</option>
							</select>
							<input
								bind:value={newHookAction}
								placeholder="Shell command to run"
								class="w-full rounded-md border border-[var(--border)] bg-[var(--background)] px-2 py-1 text-xs text-[var(--foreground)] focus:outline-none"
							/>
							<button
								onclick={handleCreateHook}
								class="w-full rounded-md bg-[var(--primary)] py-1 text-xs font-medium text-white"
								>Create</button
							>
						</div>
					{/if}
					{#each hooks as hook}
						<div class="rounded-lg bg-[var(--secondary)] p-2.5 group">
							<div class="flex items-center justify-between mb-1">
								<span
									class="text-xs font-medium text-[var(--foreground)] {!hook.enabled
										? 'line-through opacity-50'
										: ''}">{hook.name}</span
								>
								<div class="flex items-center gap-1">
									<button
										onclick={() => handleToggleHook(hook)}
										class="rounded px-1.5 py-0.5 text-[9px] {hook.enabled
											? 'bg-success-900/30 text-success-400'
											: 'bg-[var(--card)] text-[var(--muted-foreground)]'}"
									>
										{hook.enabled ? 'on' : 'off'}
									</button>
									<button
										onclick={() => handleDeleteHook(hook.id)}
										class="hidden group-hover:block text-[var(--muted-foreground)] hover:text-danger-400 text-[10px]"
										>x</button
									>
								</div>
							</div>
							<p class="text-[10px] text-[var(--muted-foreground)]">
								<span class="font-mono">{hook.event}</span> → {hook.action.slice(0, 50)}
							</p>
						</div>
					{/each}
					{#if hooks.length === 0 && !showCreateHook}
						<p class="text-center text-[10px] text-[var(--muted-foreground)] py-2">
							No hooks. Hooks automate lifecycle events.
						</p>
					{/if}
				</div>

				<!-- PROJECTS / DIRS -->
			{:else if activeTab === 'projects'}
				<div class="p-3 space-y-2">
					<p class="text-[10px] text-[var(--muted-foreground)]">Working directories (--add-dir).</p>
					{#if config}
						{#each config.add_dirs as dir}
							<div
								class="flex items-center justify-between rounded-lg bg-[var(--secondary)] px-3 py-2"
							>
								<span class="text-xs font-mono text-[var(--foreground)]">{dir}</span>
								<button
									onclick={() => removeDir(dir)}
									class="text-[var(--muted-foreground)] hover:text-danger-400 text-xs">x</button
								>
							</div>
						{/each}
						<button
							onclick={addDir}
							class="w-full rounded-lg border border-dashed border-[var(--border)] py-2 text-xs text-[var(--muted-foreground)] hover:text-[var(--foreground)]"
						>
							+ Add directory
						</button>
					{/if}
				</div>

				<!-- EVENTS -->
			{:else if activeTab === 'events'}
				<div class="p-3 space-y-2">
					{#if events.length === 0}
						<p class="text-xs text-[var(--muted-foreground)] text-center py-4">No events yet.</p>
					{:else}
						{#each events as event}
							<div class="rounded-lg bg-[var(--secondary)] p-2">
								<div class="flex items-center gap-1.5">
									<span class="rounded {cc.badge} px-1 py-0.5 text-[9px] font-mono {cc.text400}"
										>{event.kind}</span
									>
									<span class="text-[9px] text-[var(--muted-foreground)]"
										>{formatTime(event.created_at)}</span
									>
								</div>
							</div>
						{/each}
					{/if}
					<button
						onclick={loadEvents}
						class="w-full rounded-md bg-[var(--secondary)] py-1.5 text-[10px] text-[var(--muted-foreground)] hover:text-[var(--foreground)]"
						>Refresh</button
					>
				</div>
			{/if}
		</div>

		<!-- Resize handle -->
		<div
			onmousedown={startResize}
			role="button"
			tabindex="0"
			class="absolute right-0 top-0 bottom-0 w-1 cursor-col-resize hover:bg-indigo-500/50 active:bg-indigo-500/70 transition-colors"
		></div>
	</div>
{/if}
