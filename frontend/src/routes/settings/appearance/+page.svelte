<script lang="ts">
	import {
		themeState,
		setThemePreference,
		type ThemePreference
	} from '$lib/design/theme.svelte';

	const options: { value: ThemePreference; label: string; hint: string }[] = [
		{
			value: 'system',
			label: 'System',
			hint: 'Match light or dark mode from your OS.'
		},
		{
			value: 'light',
			label: 'Light',
			hint: 'Always use the light palette.'
		},
		{
			value: 'dark',
			label: 'Dark',
			hint: 'Always use the dark palette.'
		}
	];

	function segBtnClass(active: boolean) {
		return active
			? 'border-primary bg-primary/15 text-foreground shadow-sm'
			: 'border-transparent text-muted-foreground hover:border-border/60 hover:bg-muted/40 hover:text-foreground';
	}

	let resolved = $derived(
		themeState.preference === 'system'
			? themeState.systemPrefersDark
				? 'dark'
				: 'light'
			: themeState.preference
	);
</script>

<div class="space-y-6">
	<div>
		<h2 class="text-lg font-semibold text-foreground">Theme</h2>
		<p class="mt-1 text-sm text-muted-foreground">
			Controls the whole UI: tokens, Tailwind <code class="rounded bg-muted px-1 py-0.5 text-xs">dark:</code> styles,
			toasts, and scrollbars. Stored in a cookie for the next visit.
		</p>
	</div>

	<div
		class="inline-flex rounded-lg border border-border bg-card p-1 shadow-sm"
		role="group"
		aria-label="Color theme"
	>
		{#each options as o}
			<button
				type="button"
				onclick={() => setThemePreference(o.value)}
				class="rounded-md border px-4 py-2 text-sm font-medium transition-colors {segBtnClass(
					themeState.preference === o.value
				)}"
				aria-pressed={themeState.preference === o.value}
			>
				{o.label}
			</button>
		{/each}
	</div>

	<p class="text-xs text-muted-foreground">
		Active appearance: <span class="font-medium text-foreground capitalize">{resolved}</span>
		{#if themeState.preference === 'system'}
			(from system)
		{/if}
	</p>

	<ul class="space-y-3 text-sm text-muted-foreground">
		{#each options as o}
			<li>
				<span class="font-medium text-foreground">{o.label}</span>
				— {o.hint}
			</li>
		{/each}
	</ul>
</div>
