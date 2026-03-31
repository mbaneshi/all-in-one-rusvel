<script lang="ts">
	import '../app.css';
	import type { Snippet } from 'svelte';
	import { onMount } from 'svelte';
	import OnboardingChecklist from '$lib/components/onboarding/OnboardingChecklist.svelte';
	import ProductTour from '$lib/components/onboarding/ProductTour.svelte';
	import CommandPalette from '$lib/components/onboarding/CommandPalette.svelte';
	import IconRail from '$lib/components/shell/IconRail.svelte';
	import TopBar from '$lib/components/shell/TopBar.svelte';
	import { Toaster } from 'svelte-sonner';
	import {
		themePreference,
		systemPrefersDark,
		initTheme
	} from '$lib/design/theme.svelte';

	let { children }: { children: Snippet } = $props();

	let toasterTheme = $derived<'light' | 'dark'>(
		themePreference === 'system'
			? systemPrefersDark
				? 'dark'
				: 'light'
			: themePreference === 'light'
				? 'light'
				: 'dark'
	);

	onMount(() => initTheme());
</script>

<div class="flex h-screen flex-col bg-background text-foreground">
	<div class="flex min-h-0 flex-1 overflow-hidden">
		<IconRail />
		<div class="flex min-h-0 min-w-0 flex-1 flex-col">
			<TopBar />
			<main class="rusvel-main-scroll min-h-0 flex-1 overflow-y-auto overflow-x-hidden scroll-smooth">
				{@render children()}
			</main>
		</div>
	</div>
</div>

<Toaster richColors position="bottom-right" theme={toasterTheme} />
<CommandPalette />
<OnboardingChecklist />
<ProductTour />
