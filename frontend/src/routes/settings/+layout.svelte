<script lang="ts">
	import type { Snippet } from 'svelte';
	import { page } from '$app/state';

	let { children }: { children: Snippet } = $props();

	const tabs = [
		{
			href: '/settings',
			label: 'Overview',
			active: (path: string) => path === '/settings' || path === '/settings/'
		},
		{
			href: '/settings/control',
			label: 'Control center',
			active: (path: string) => path.startsWith('/settings/control')
		},
		{
			href: '/settings/llm',
			label: 'LLM & models',
			active: (path: string) => path.startsWith('/settings/llm')
		},
		{
			href: '/settings/spend',
			label: 'Spend',
			active: (path: string) => path.startsWith('/settings/spend')
		}
	] as const;
</script>

<div class="border-b border-border bg-gradient-to-b from-card/80 to-background">
	<div class="mx-auto max-w-4xl px-4 pt-6 sm:px-6">
		<p class="text-[11px] font-medium uppercase tracking-widest text-muted-foreground">Workspace</p>
		<h1 class="mt-1 text-2xl font-bold tracking-tight text-foreground">Settings</h1>
		<p class="mt-1 max-w-2xl text-sm text-muted-foreground leading-relaxed">
			Configure models, provider health, and integrations. Subpages load their own data.
		</p>
		<nav
			class="mt-6 flex gap-1 overflow-x-auto border-b border-border pb-px [-ms-overflow-style:none] [scrollbar-width:none] [&::-webkit-scrollbar]:hidden"
			aria-label="Settings sections"
		>
			{#each tabs as t}
				<a
					href={t.href}
					class="shrink-0 rounded-t-md border border-b-0 px-3 py-2 text-sm font-medium transition-colors {t.active(
						page.url.pathname
					)
						? 'border-border bg-card text-foreground shadow-sm'
						: 'border-transparent text-muted-foreground hover:border-border/60 hover:bg-muted/30 hover:text-foreground'}"
				>
					{t.label}
				</a>
			{/each}
		</nav>
	</div>
</div>

<div class="mx-auto max-w-4xl px-4 py-6 pb-32 sm:px-6">
	{@render children()}
</div>
