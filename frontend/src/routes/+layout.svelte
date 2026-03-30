<script lang="ts">
	import '../app.css';
	import type { Snippet } from 'svelte';
	import OnboardingChecklist from '$lib/components/onboarding/OnboardingChecklist.svelte';
	import ProductTour from '$lib/components/onboarding/ProductTour.svelte';
	import CommandPalette from '$lib/components/onboarding/CommandPalette.svelte';
	import IconRail from '$lib/components/shell/IconRail.svelte';
	import TopBar from '$lib/components/shell/TopBar.svelte';
	import { Toaster } from 'svelte-sonner';

	let { children }: { children: Snippet } = $props();
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

<Toaster richColors position="bottom-right" theme="dark" />
<CommandPalette />
<OnboardingChecklist />
<ProductTour />
